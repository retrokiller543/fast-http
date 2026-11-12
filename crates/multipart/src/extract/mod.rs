use bytes::{Buf, Bytes};
use fast_http_core::{BoxError, FromRequest, ReqBody};
use futures_util::{Stream, TryStreamExt};
use http::{header::CONTENT_TYPE, request::Parts};
use http_body_util::BodyExt;
use multer::{Constraints, SizeLimit};

use crate::{Field, MultipartError};

/// Default limit for the whole multipart body, 8 MiB.
pub const DEFAULT_WHOLE_LIMIT: usize = fast_http_core::DEFAULT_BODY_LIMIT;

/// Default limit for one field, 8 MiB: a single upload may use the whole body.
pub const DEFAULT_FIELD_LIMIT: usize = fast_http_core::DEFAULT_BODY_LIMIT;

/// A `multipart/form-data` request body, read field by field.
///
/// `WHOLE` caps the entire body and `FIELD` caps each field, both in bytes; exceeding either
/// yields [`MultipartError::TooLarge`]. The server's own body limit applies as well and must be
/// raised for large uploads.
#[derive(Debug)]
pub struct Multipart<
    const WHOLE: usize = DEFAULT_WHOLE_LIMIT,
    const FIELD: usize = DEFAULT_FIELD_LIMIT,
> {
    inner: multer::Multipart<'static>,
}

impl<const WHOLE: usize, const FIELD: usize> Multipart<WHOLE, FIELD> {
    /// Reads the next field, or `None` after the last one.
    pub async fn next_field(&mut self) -> Result<Option<Field>, MultipartError> {
        self.inner
            .next_field()
            .await
            .map(|field| field.map(Field::new))
            .map_err(MultipartError::from_multer)
    }
}

fn data_stream<B: ReqBody>(body: B) -> impl Stream<Item = Result<Bytes, BoxError>> + Send {
    body.into_data_stream()
        .map_ok(|mut data| data.copy_to_bytes(data.remaining()))
        .map_err(Into::into)
}

impl<const WHOLE: usize, const FIELD: usize> FromRequest for Multipart<WHOLE, FIELD> {
    type Rejection = MultipartError;

    async fn from_request<B: ReqBody>(parts: &Parts, body: B) -> Result<Self, Self::Rejection> {
        let content_type = parts
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .ok_or(MultipartError::NotMultipart)?;
        let boundary = multer::parse_boundary(content_type).map_err(MultipartError::from_multer)?;
        let limits = SizeLimit::new()
            .whole_stream(WHOLE as u64)
            .per_field(FIELD as u64);
        let constraints = Constraints::new().size_limit(limits);

        Ok(Self {
            inner: multer::Multipart::with_constraints(data_stream(body), boundary, constraints),
        })
    }
}

#[cfg(test)]
mod tests;
