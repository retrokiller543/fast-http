use std::marker::PhantomData;

use fast_http_core::{IntoOutcome, Outcome, Response, ResponseBody, media::negotiate};
use http::{
    HeaderValue, StatusCode,
    header::{ACCEPT, CONTENT_TYPE, VARY},
    request::Parts,
};

use crate::{
    Enabled, EncodeSet, FormatError,
    wrapper::{Wrapper, transparent},
};

/// Response value encoded in whichever format of `S` the `Accept` header prefers.
///
/// Defaults to every enabled format; name a set to restrict it, such as `Negotiated<T, (Json,
/// Yaml)>`, or to add custom formats. For exactly one format whatever the client accepts, return
/// that format's own wrapper, such as `Json<T>`.
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

impl<T, S> Wrapper for Negotiated<T, S> {
    type Inner = T;

    fn get(&self) -> &T {
        &self.value
    }

    fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    fn unwrap(self) -> T {
        self.value
    }

    fn wrap(value: T) -> Self {
        Self::new(value)
    }
}

transparent!(access Negotiated [T, S] [T, S]);

impl<T: std::fmt::Debug, S> std::fmt::Debug for Negotiated<T, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Negotiated")
            .field("value", &self.value)
            .field("status", &self.status)
            .finish()
    }
}

impl<T: Clone, S> Clone for Negotiated<T, S> {
    fn clone(&self) -> Self {
        Self::new(self.value.clone()).status(self.status)
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

#[cfg(all(test, feature = "json"))]
mod tests;
