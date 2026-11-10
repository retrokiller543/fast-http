use std::borrow::Cow;

use fast_http_core::{FromParts, Response, ResponseBody, TaskSpawner};
use http::{
    HeaderValue, Method, StatusCode,
    header::{
        CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_PROTOCOL,
        SEC_WEBSOCKET_VERSION, UPGRADE,
    },
    request::Parts,
};
use hyper::upgrade::OnUpgrade;
use hyper_util::rt::TokioIo;
use tokio_tungstenite::tungstenite::{handshake::derive_accept_key, protocol::WebSocketConfig};

use crate::{
    Admission, AdmissionRejection, Permit, TRACE_TARGET, WebSocket, WebSocketUpgradeRejection,
    guard::has_token,
};

/// A validated WebSocket handshake, ready to be answered with [`on_upgrade`](Self::on_upgrade).
///
/// Extracting it checks the RFC 6455 handshake headers, so a handler that takes one never runs
/// for a request that is not a WebSocket upgrade. Take `Option<WebSocketUpgrade>` to serve a
/// plain response on the same path instead.
#[derive(Debug)]
pub struct WebSocketUpgrade {
    config: WebSocketConfig,
    accept: HeaderValue,
    requested: Vec<String>,
    selected: Option<HeaderValue>,
    on_upgrade: OnUpgrade,
    spawner: Option<TaskSpawner>,
    permit: Option<Permit>,
}

impl WebSocketUpgrade {
    /// Takes a connection slot from `admission`, held until the socket is dropped.
    ///
    /// Fails with an [`AdmissionRejection`], which renders as `503`, when the limit is reached or
    /// the endpoint is shutting down, before anything is upgraded.
    pub fn admit(mut self, admission: &Admission) -> Result<Self, AdmissionRejection> {
        self.permit = Some(admission.try_admit()?);

        Ok(self)
    }

    /// Largest accepted message in bytes; larger ones fail the read with a size error.
    pub fn max_message_size(mut self, max: usize) -> Self {
        self.config = self.config.max_message_size(Some(max));

        self
    }

    /// Largest accepted frame in bytes.
    pub fn max_frame_size(mut self, max: usize) -> Self {
        self.config = self.config.max_frame_size(Some(max));

        self
    }

    /// Size of the read buffer in bytes.
    pub fn read_buffer_size(mut self, size: usize) -> Self {
        self.config = self.config.read_buffer_size(size);

        self
    }

    /// Size at which queued outgoing data is written to the socket.
    pub fn write_buffer_size(mut self, size: usize) -> Self {
        self.config = self.config.write_buffer_size(size);

        self
    }

    /// Upper bound of the outgoing queue; sends fail beyond it instead of buffering.
    pub fn max_write_buffer_size(mut self, size: usize) -> Self {
        self.config = self.config.max_write_buffer_size(size);

        self
    }

    /// Offers `protocols` in server preference order and selects the first the client also
    /// requested, see [`selected_protocol`](Self::selected_protocol).
    pub fn protocols<I>(mut self, protocols: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Cow<'static, str>>,
    {
        self.selected = protocols
            .into_iter()
            .map(Into::into)
            .find(|offered| self.requested.iter().any(|asked| asked == offered.as_ref()))
            .and_then(|chosen| HeaderValue::from_str(&chosen).ok());

        self
    }

    /// The subprotocol that [`protocols`](Self::protocols) selected, if any.
    pub fn selected_protocol(&self) -> Option<&HeaderValue> {
        self.selected.as_ref()
    }

    /// Answers the handshake with `101 Switching Protocols` and, once the response is sent, runs
    /// `callback` with the upgraded connection on a spawned task.
    ///
    /// The task is spawned through the server's `TaskWrap`, from the handler, so it runs in the
    /// same context as the request. Outside a server, which provides no spawner, it is spawned
    /// directly on tokio.
    pub fn on_upgrade<F, Fut>(self, callback: F) -> Response
    where
        F: FnOnce(WebSocket) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let Self {
            config,
            accept,
            selected,
            on_upgrade,
            spawner,
            permit,
            ..
        } = self;
        let protocol = selected.clone();
        let mut response = Response::new(ResponseBody::Empty);

        let task = async move {
            match on_upgrade.await {
                Ok(upgraded) => {
                    let socket = WebSocket::from_io(TokioIo::new(upgraded), protocol, config).await;

                    callback(match permit {
                        Some(permit) => socket.with_permit(permit),
                        None => socket,
                    })
                    .await;
                }
                Err(error) => {
                    tracing::warn!(target: TRACE_TARGET, %error, "websocket upgrade failed");
                }
            }
        };

        match spawner {
            Some(spawner) => spawner.spawn(task),
            None => {
                tokio::spawn(task);
            }
        }

        *response.status_mut() = StatusCode::SWITCHING_PROTOCOLS;

        let headers = response.headers_mut();

        headers.insert(CONNECTION, HeaderValue::from_static("upgrade"));
        headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
        headers.insert(SEC_WEBSOCKET_ACCEPT, accept);

        if let Some(protocol) = selected {
            headers.insert(SEC_WEBSOCKET_PROTOCOL, protocol);
        }

        response
    }
}

fn requested_protocols(parts: &Parts) -> Vec<String> {
    parts
        .headers
        .get_all(SEC_WEBSOCKET_PROTOCOL)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
        .collect()
}

impl FromParts for WebSocketUpgrade {
    type Rejection = WebSocketUpgradeRejection;

    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection> {
        if parts.method != Method::GET {
            return Err(WebSocketUpgradeRejection::MethodNotGet);
        }

        if !has_token(&parts.headers, &CONNECTION, "upgrade") {
            return Err(WebSocketUpgradeRejection::InvalidConnection);
        }

        if !has_token(&parts.headers, &UPGRADE, "websocket") {
            return Err(WebSocketUpgradeRejection::InvalidUpgrade);
        }

        if parts.headers.get(SEC_WEBSOCKET_VERSION) != Some(&HeaderValue::from_static("13")) {
            return Err(WebSocketUpgradeRejection::UnsupportedVersion);
        }

        let key = parts
            .headers
            .get(SEC_WEBSOCKET_KEY)
            .ok_or(WebSocketUpgradeRejection::MissingKey)?;
        let accept = HeaderValue::from_str(&derive_accept_key(key.as_bytes()))
            .map_err(|_| WebSocketUpgradeRejection::InvalidKey)?;
        let on_upgrade = parts
            .extensions
            .get::<OnUpgrade>()
            .cloned()
            .ok_or(WebSocketUpgradeRejection::NotUpgradable)?;

        Ok(Self {
            config: WebSocketConfig::default(),
            accept,
            requested: requested_protocols(parts),
            selected: None,
            on_upgrade,
            spawner: TaskSpawner::current(),
            permit: None,
        })
    }
}

#[cfg(test)]
mod tests;
