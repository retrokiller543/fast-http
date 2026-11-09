use std::marker::PhantomData;

use fast_http_core::{IntoOutcome, Outcome, Response, ResponseBody, media::negotiate};
use http::{
    HeaderValue, StatusCode,
    header::{ACCEPT, CONTENT_TYPE, VARY},
    request::Parts,
};

use crate::{Enabled, EncodeSet, FormatError};

/// Response value encoded in whichever format of `S` the `Accept` header prefers.
///
/// Defaults to every enabled format; name a set to restrict it or to add custom formats.
pub struct Negotiated<T, S = Enabled> {
    /// The value to encode.
    pub value: T,
    status: StatusCode,
    set: PhantomData<fn() -> S>,
}

impl<T, S> Negotiated<T, S> {
    /// Wraps a value, answering with `200 OK`.
    pub fn new(value: T) -> Self {
        Self {
            value,
            status: StatusCode::OK,
            set: PhantomData,
        }
    }

    /// Overrides the response status.
    pub fn status(mut self, status: StatusCode) -> Self {
        self.status = status;

        self
    }
}

impl<T, S: EncodeSet<T>> IntoOutcome for Negotiated<T, S> {
    fn into_outcome(self, parts: &Parts) -> Outcome {
        let accept = parts
            .headers
            .get(ACCEPT)
            .and_then(|value| value.to_str().ok());
        let index = negotiate(accept, S::LEN, S::media_type).ok_or(FormatError::NotAcceptable)?;
        let bytes = S::encode(index, &self.value, parts)?;
        let mut response = Response::new(ResponseBody::Bytes(bytes));

        *response.status_mut() = self.status;
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static(S::media_type(index).essence()),
        );
        response
            .headers_mut()
            .append(VARY, HeaderValue::from_static("accept"));

        Ok(response)
    }
}

#[cfg(test)]
mod tests;
