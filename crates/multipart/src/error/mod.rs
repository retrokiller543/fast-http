use fast_http_core::{BoxError, ErrorResponse, IntoResponse, Response, ResponseBody};
use http::StatusCode;
use http_body_util::LengthLimitError;

/// Why a multipart request could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MultipartError {
    /// The request is not `multipart/form-data`.
    #[error("request is not multipart/form-data")]
    NotMultipart,

    /// The `Content-Type` carries no boundary parameter.
    #[error("multipart content type has no boundary")]
    MissingBoundary,

    /// A field, or the whole body, exceeded its size limit.
    #[error("multipart body exceeds its size limit")]
    TooLarge,

    /// The body is not valid multipart or could not be read.
    #[error("invalid multipart body: {0}")]
    Malformed(BoxError),
}

impl MultipartError {
    pub(crate) fn from_multer(error: multer::Error) -> Self {
        match error {
            multer::Error::FieldSizeExceeded { .. } | multer::Error::StreamSizeExceeded { .. } => {
                Self::TooLarge
            }
            multer::Error::StreamReadFailed(source) if source.is::<LengthLimitError>() => {
                Self::TooLarge
            }
            multer::Error::NoMultipart => Self::NotMultipart,
            multer::Error::NoBoundary => Self::MissingBoundary,
            other => Self::Malformed(Box::new(other)),
        }
    }
}

impl IntoResponse for MultipartError {
    fn into_response(self) -> Response {
        let message = match self {
            Self::NotMultipart => "request is not multipart/form-data",
            Self::MissingBoundary => "multipart content type has no boundary",
            Self::TooLarge => "multipart body exceeds its size limit",
            Self::Malformed(_) => "invalid multipart body",
        };
        let mut response = Response::new(ResponseBody::from(message));

        *response.status_mut() = ErrorResponse::status(&self);

        response
    }
}

impl ErrorResponse for MultipartError {
    fn status(&self) -> StatusCode {
        match self {
            Self::NotMultipart => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::MissingBoundary | Self::Malformed(_) => StatusCode::BAD_REQUEST,
        }
    }
}

#[cfg(test)]
mod tests;
