use bytes::Bytes;
use http::HeaderMap;

use crate::MultipartError;

/// One part of a multipart body, read incrementally.
///
/// A field borrows nothing from its [`Multipart`](crate::Multipart), but the parser only moves
/// on once the field is dropped: drop a field read through [`chunk`](Self::chunk) before calling
/// `next_field`, even after `chunk` returned `None`, or `next_field` fails. [`bytes`](Self::bytes)
/// and [`text`](Self::text) consume the field, so nothing more is needed after them.
#[derive(Debug)]
pub struct Field {
    inner: multer::Field<'static>,
}

impl Field {
    pub(crate) fn new(inner: multer::Field<'static>) -> Self {
        Self { inner }
    }

    /// The form field name from `Content-Disposition`.
    pub fn name(&self) -> Option<&str> {
        self.inner.name()
    }

    /// The uploaded file name, for file fields.
    pub fn file_name(&self) -> Option<&str> {
        self.inner.file_name()
    }

    /// The part's own `Content-Type`.
    pub fn content_type(&self) -> Option<&str> {
        self.inner.content_type().map(AsRef::as_ref)
    }

    /// All headers of the part.
    pub fn headers(&self) -> &HeaderMap {
        self.inner.headers()
    }

    /// Reads the rest of the field into memory.
    pub async fn bytes(self) -> Result<Bytes, MultipartError> {
        self.inner
            .bytes()
            .await
            .map_err(MultipartError::from_multer)
    }

    /// Reads the rest of the field as text, decoding by the part's charset (UTF-8 by default).
    pub async fn text(self) -> Result<String, MultipartError> {
        self.inner.text().await.map_err(MultipartError::from_multer)
    }

    /// Reads the next chunk of the field, or `None` at its end.
    pub async fn chunk(&mut self) -> Result<Option<Bytes>, MultipartError> {
        self.inner
            .chunk()
            .await
            .map_err(MultipartError::from_multer)
    }
}
