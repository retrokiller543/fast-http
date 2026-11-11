use std::marker::PhantomData;

use fast_http_core::{FromRequest, Rejection, ReqBody, buffer, media::match_content_type};
use http::{header::CONTENT_TYPE, request::Parts};

use crate::{
    DecodeSet, Enabled, FormatError,
    wrapper::{Wrapper, transparent},
};

/// Default request body limit, 8 MiB.
pub const DEFAULT_BODY_LIMIT: usize = fast_http_core::DEFAULT_BODY_LIMIT;

/// Request value decoded by the format of `S` that matches `Content-Type`, buffering at most `LIMIT` bytes.
///
/// Defaults to every enabled format; name a set to restrict it, such as `Body<T, (Json, Yaml)>`,
/// or to add custom formats. For exactly one format, take that format's own wrapper, such as
/// `Json<T>`. The value is reached through `Deref` (`body.field`), or moved out with
/// `into_inner`.
pub struct Body<T, S = Enabled, const LIMIT: usize = DEFAULT_BODY_LIMIT> {
    /// The decoded value.
    value: T,
    set: PhantomData<fn() -> S>,
}

impl<T, S, const LIMIT: usize> Wrapper for Body<T, S, LIMIT> {
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
        Self {
            value,
            set: PhantomData,
        }
    }
}

transparent!(value Body [T, S, const LIMIT: usize] [T, S, LIMIT]);

impl<T, S, const LIMIT: usize> FromRequest for Body<T, S, LIMIT>
where
    T: Send,
    S: DecodeSet<T>,
{
    type Rejection = FormatError;

    async fn from_request<B: ReqBody>(parts: &Parts, body: B) -> Result<Self, Self::Rejection> {
        let content_type = parts
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .ok_or(FormatError::MissingContentType)?;
        let index = match_content_type(content_type, S::LEN, S::media_type)
            .ok_or(FormatError::UnsupportedMediaType)?;
        let bytes = buffer(body, LIMIT)
            .await
            .map_err(|rejection| match rejection {
                Rejection::BodyTooLarge { limit } => FormatError::TooLarge { limit },
                other => FormatError::Body(other.to_string().into()),
            })?;
        let value = S::decode(index, &bytes)?;

        Ok(Self::wrap(value))
    }
}

#[cfg(all(test, feature = "json"))]
mod tests;
