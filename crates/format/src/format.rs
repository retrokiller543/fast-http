use bytes::Bytes;
use fast_http_core::MediaType;
use http::request::Parts;

use crate::FormatError;

/// A wire format identified by its media type; implement this plus [`Encode`] and/or [`Decode`] for your own.
pub trait Format: 'static {
    /// Media type used for `Content-Type` and `Accept` matching.
    const MEDIA_TYPE: MediaType;
}

/// Serializes a `T`; the request head is available for per-request concerns such as localisation.
pub trait Encode<T>: Format {
    /// Renders `value` as bytes.
    fn encode(value: &T, parts: &Parts) -> Result<Bytes, FormatError>;
}

/// Deserializes a `T` from a fully buffered body.
pub trait Decode<T>: Format {
    /// Parses `bytes` into a value.
    fn decode(bytes: &[u8]) -> Result<T, FormatError>;
}
