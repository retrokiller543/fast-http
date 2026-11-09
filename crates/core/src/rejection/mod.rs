use http::{Method, StatusCode, header};

use crate::{ErrorResponse, IntoResponse, Response, ResponseBody};

/// Typed routing and extraction failures; statuses are plain `http::StatusCode`s.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    /// No controller or endpoint owns the path.
    #[error("not found")]
    NotFound,

    /// The path exists but not for this method.
    #[error("method not allowed")]
    MethodNotAllowed {
        /// Methods the path does accept, derived at compile time.
        allow: Vec<Method>,
    },

    /// The path and method exist but every guard failed.
    #[error("no endpoint variant matched the request")]
    GuardMiss {
        /// Configurable status, commonly 400, 404 or 422.
        status: StatusCode,
    },

    /// Reading the request body failed.
    #[error("failed to read request body: {message}")]
    BodyRead {
        /// Description of the underlying failure.
        message: String,
    },

    /// A path parameter could not be parsed.
    #[error("invalid path parameter `{name}`: {message}")]
    BadPathParam {
        /// Parameter name as declared on the route.
        name: &'static str,
        /// Description of the parse failure.
        message: String,
    },

    /// An extractor needed a request extension that no layer inserted.
    #[error("missing request extension `{name}`")]
    MissingExtension {
        /// Type name of the missing extension.
        name: &'static str,
    },

    /// The buffered body exceeded its limit.
    #[error("request body exceeds {limit} bytes")]
    BodyTooLarge {
        /// Limit in bytes that was exceeded.
        limit: usize,
    },
}

impl Rejection {
    /// Status code this rejection renders as.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::MethodNotAllowed { .. } => StatusCode::METHOD_NOT_ALLOWED,
            Self::GuardMiss { status } => *status,
            Self::BodyTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::BodyRead { .. } | Self::BadPathParam { .. } => StatusCode::BAD_REQUEST,
            Self::MissingExtension { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl Rejection {
    /// Replaces the status of a guard miss, leaving other rejections untouched.
    pub fn with_guard_miss(self, status: StatusCode) -> Self {
        match self {
            Self::GuardMiss { .. } => Self::GuardMiss { status },
            other => other,
        }
    }
}

impl IntoResponse for Rejection {
    fn into_response(self) -> Response {
        let mut response = Response::new(ResponseBody::from(self.to_string()));

        *response.status_mut() = self.status();

        if let Self::MethodNotAllowed { allow } = &self {
            let value = allow
                .iter()
                .map(Method::as_str)
                .collect::<Vec<_>>()
                .join(", ");

            if let Ok(value) = header::HeaderValue::from_str(&value) {
                response.headers_mut().insert(header::ALLOW, value);
            }
        }

        response
    }
}

impl ErrorResponse for Rejection {
    fn status(&self) -> StatusCode {
        Rejection::status(self)
    }
}

#[cfg(test)]
mod tests;
