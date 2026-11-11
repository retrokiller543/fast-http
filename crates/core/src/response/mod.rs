use std::convert::Infallible;

use bytes::Bytes;
use http::StatusCode;

use crate::{Response, ResponseBody};

/// Converts a handler return value into a [`Response`].
pub trait IntoResponse {
    /// Performs the conversion.
    fn into_response(self) -> Response;
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl IntoResponse for ResponseBody {
    fn into_response(self) -> Response {
        Response::new(self)
    }
}

impl IntoResponse for Infallible {
    fn into_response(self) -> Response {
        match self {}
    }
}

impl IntoResponse for () {
    fn into_response(self) -> Response {
        Response::new(ResponseBody::Empty)
    }
}

impl IntoResponse for StatusCode {
    fn into_response(self) -> Response {
        let mut response = Response::new(ResponseBody::Empty);

        *response.status_mut() = self;

        response
    }
}

impl IntoResponse for &str {
    fn into_response(self) -> Response {
        ResponseBody::from(Bytes::copy_from_slice(self.as_bytes())).into_response()
    }
}

impl IntoResponse for String {
    fn into_response(self) -> Response {
        ResponseBody::from(self).into_response()
    }
}

impl IntoResponse for Bytes {
    fn into_response(self) -> Response {
        ResponseBody::from(self).into_response()
    }
}

impl IntoResponse for Vec<u8> {
    fn into_response(self) -> Response {
        ResponseBody::from(self).into_response()
    }
}

impl<T: IntoResponse> IntoResponse for (StatusCode, T) {
    fn into_response(self) -> Response {
        let (status, inner) = self;
        let mut response = inner.into_response();

        *response.status_mut() = status;

        response
    }
}

#[cfg(test)]
mod tests;
