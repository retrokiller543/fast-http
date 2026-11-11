use std::sync::Arc;

use fast_http_core::{ErrorResponse, IntoResponse, Response, ResponseBody};
use http::{HeaderValue, StatusCode, header::RETRY_AFTER};
use tokio::sync::watch;

/// Caps the live WebSocket connections of an endpoint and tracks them until they end.
///
/// Admission happens before the upgrade, so a client over the limit gets a plain `503` instead
/// of a socket. The [`Permit`] lives as long as the connection, so [`active`](Self::active) is
/// the number of open sockets, [`shutdown`](Self::shutdown) tells every one of them to wrap up
/// and [`drained`](Self::drained) waits until the last is gone. Clones share one state.
#[derive(Debug, Clone)]
pub struct Admission(Arc<Inner>);

#[derive(Debug)]
struct Inner {
    limit: Option<usize>,
    active: watch::Sender<usize>,
    closing: watch::Sender<bool>,
}

impl Admission {
    /// Admits at most `limit` concurrent connections; `0` admits none.
    pub fn new(limit: usize) -> Self {
        Self::with_limit(Some(limit))
    }

    /// Admits any number of connections but still counts them and carries shutdown.
    pub fn unlimited() -> Self {
        Self::with_limit(None)
    }

    fn with_limit(limit: Option<usize>) -> Self {
        Self(Arc::new(Inner {
            limit,
            active: watch::Sender::new(0),
            closing: watch::Sender::new(false),
        }))
    }

    /// The connection cap, or `None` when unlimited.
    pub fn limit(&self) -> Option<usize> {
        self.0.limit
    }

    /// Connections admitted and not yet ended.
    pub fn active(&self) -> usize {
        *self.0.active.borrow()
    }

    /// Reserves a slot for one connection, held until the returned [`Permit`] is dropped.
    ///
    /// The shutdown check and the count happen under the same lock that [`shutdown`](Self::shutdown)
    /// takes, so an admission either completes before shutdown, and [`drained`](Self::drained)
    /// waits for it, or sees the shutdown and is refused.
    pub fn try_admit(&self) -> Result<Permit, AdmissionRejection> {
        let limit = self.0.limit;
        let mut outcome = Ok(());

        self.0.active.send_if_modified(|active| {
            if *self.0.closing.borrow() {
                outcome = Err(AdmissionRejection::ShuttingDown);

                return false;
            }

            if limit.is_some_and(|limit| *active >= limit) {
                outcome = Err(AdmissionRejection::AtCapacity {
                    limit: limit.unwrap_or_default(),
                });

                return false;
            }

            *active += 1;

            true
        });

        outcome.map(|()| Permit {
            admission: self.clone(),
        })
    }

    /// Stops admitting and signals every live connection through its [`ShutdownSignal`].
    pub fn shutdown(&self) {
        self.0.active.send_if_modified(|_| {
            self.0.closing.send_replace(true);

            false
        });
    }

    /// Whether [`shutdown`](Self::shutdown) was called.
    pub fn is_shutting_down(&self) -> bool {
        *self.0.closing.borrow()
    }

    /// Resolves once no admitted connection is left.
    pub async fn drained(&self) {
        let mut active = self.0.active.subscribe();

        let _ = active.wait_for(|active| *active == 0).await;
    }

    fn signal(&self) -> ShutdownSignal {
        ShutdownSignal(Some(self.0.closing.subscribe()))
    }
}

/// One admitted connection; dropping it frees the slot.
#[derive(Debug)]
pub struct Permit {
    admission: Admission,
}

impl Permit {
    /// A signal that fires when the [`Admission`] starts shutting down.
    pub fn shutdown_signal(&self) -> ShutdownSignal {
        self.admission.signal()
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.admission.0.active.send_modify(|active| *active -= 1);
    }
}

/// Resolves when the endpoint a connection was admitted by starts shutting down.
///
/// Independent of the socket, so it can sit next to `recv` in a `select!`. A connection that was
/// not admitted through an [`Admission`] gets a signal that never fires.
#[derive(Debug, Clone, Default)]
pub struct ShutdownSignal(Option<watch::Receiver<bool>>);

impl ShutdownSignal {
    /// Waits for shutdown; cancel safe, and ready at once if shutdown already started.
    pub async fn wait(&mut self) {
        match &mut self.0 {
            Some(closing) => {
                let _ = closing.wait_for(|closing| *closing).await;
            }
            None => std::future::pending().await,
        }
    }
}

/// Why a connection was not admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AdmissionRejection {
    /// The connection limit is reached.
    #[error("websocket connection limit of {limit} reached")]
    AtCapacity {
        /// The configured limit.
        limit: usize,
    },

    /// The endpoint is shutting down and admits no new connections.
    #[error("websocket endpoint is shutting down")]
    ShuttingDown,
}

impl IntoResponse for AdmissionRejection {
    fn into_response(self) -> Response {
        let mut response = Response::new(ResponseBody::from(self.to_string()));

        *response.status_mut() = ErrorResponse::status(&self);

        if matches!(self, Self::AtCapacity { .. }) {
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from_static("1"));
        }

        response
    }
}

impl ErrorResponse for AdmissionRejection {
    fn status(&self) -> StatusCode {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

#[cfg(test)]
mod tests;
