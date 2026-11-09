use bytes::Bytes;
use fast_http_core::{BoxError, ErrorResponse, IntoResponse, Response, ResponseBody};
use http::StatusCode;

/// Failures while choosing a format, or while encoding or decoding with one.
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    /// No offered format satisfies the `Accept` header.
    #[error("no acceptable representation for the requested media types")]
    NotAcceptable,

    /// The request body's `Content-Type` is not one of the accepted formats.
    #[error("unsupported content type")]
    UnsupportedMediaType,

    /// The request has a body to decode but no `Content-Type`.
    #[error("missing content type")]
    MissingContentType,

    /// Reading the request body failed.
    #[error("failed to read request body: {0}")]
    Body(BoxError),

    /// The request body exceeded its limit.
    #[error("request body exceeds {limit} bytes")]
    TooLarge {
        /// Limit in bytes that was exceeded.
        limit: usize,
    },

    /// The body is not valid for the chosen format and type.
    #[error("invalid request body: {0}")]
    Decode(BoxError),

    /// Serializing the response failed.
    #[error("failed to encode response: {0}")]
    Encode(BoxError),
}

impl FormatError {
    /// Wraps a format-specific decode error.
    pub fn decode(error: impl Into<BoxError>) -> Self {
        Self::Decode(error.into())
    }

    /// Wraps a format-specific encode error.
    pub fn encode(error: impl Into<BoxError>) -> Self {
        Self::Encode(error.into())
    }
}

impl IntoResponse for FormatError {
    fn into_response(self) -> Response {
        let message = match &self {
            Self::Encode(_) => "failed to encode response".to_owned(),
            other => other.to_string(),
        };
        let mut response = Response::new(ResponseBody::Bytes(Bytes::from(message)));

        *response.status_mut() = ErrorResponse::status(&self);

        response
    }
}

impl ErrorResponse for FormatError {
    fn status(&self) -> StatusCode {
        match self {
            Self::NotAcceptable => StatusCode::NOT_ACCEPTABLE,
            Self::UnsupportedMediaType | Self::MissingContentType => {
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            }
            Self::Body(_) | Self::Decode(_) => StatusCode::BAD_REQUEST,
            Self::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Encode(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
