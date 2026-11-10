use fast_http_core::{ErrorResponse, IntoResponse, Response, ResponseBody};
use http::{HeaderValue, StatusCode, header};

/// Why a request could not be upgraded to a WebSocket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WebSocketUpgradeRejection {
    /// The handshake must be a `GET` request.
    #[error("websocket upgrade requires the GET method")]
    MethodNotGet,

    /// The `Connection` header does not contain `upgrade`.
    #[error("missing `Connection: upgrade` header")]
    InvalidConnection,

    /// The `Upgrade` header is missing or is not `websocket`.
    #[error("missing `Upgrade: websocket` header")]
    InvalidUpgrade,

    /// `Sec-WebSocket-Version` is missing or is not 13.
    #[error("unsupported websocket version, expected 13")]
    UnsupportedVersion,

    /// The `Sec-WebSocket-Key` header is missing.
    #[error("missing `Sec-WebSocket-Key` header")]
    MissingKey,

    /// The `Sec-WebSocket-Key` header cannot be answered.
    #[error("invalid `Sec-WebSocket-Key` header")]
    InvalidKey,

    /// The connection carries no upgrade handle, such as an HTTP/2 stream.
    #[error("the connection cannot be upgraded")]
    NotUpgradable,
}

impl IntoResponse for WebSocketUpgradeRejection {
    fn into_response(self) -> Response {
        let mut response = Response::new(ResponseBody::from(self.to_string()));

        *response.status_mut() = ErrorResponse::status(&self);

        match self {
            Self::MethodNotGet => {
                response
                    .headers_mut()
                    .insert(header::ALLOW, HeaderValue::from_static("GET"));
            }
            Self::UnsupportedVersion => {
                response.headers_mut().insert(
                    header::SEC_WEBSOCKET_VERSION,
                    HeaderValue::from_static("13"),
                );
            }
            _ => {}
        }

        response
    }
}

impl ErrorResponse for WebSocketUpgradeRejection {
    fn status(&self) -> StatusCode {
        match self {
            Self::MethodNotGet => StatusCode::METHOD_NOT_ALLOWED,
            Self::UnsupportedVersion => StatusCode::UPGRADE_REQUIRED,
            Self::NotUpgradable => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_REQUEST,
        }
    }
}

#[cfg(test)]
mod tests;
