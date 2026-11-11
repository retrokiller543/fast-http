use fast_http_core::FromParts;
use http::request::Parts;
use serde::de::DeserializeOwned;

use crate::FormatError;

/// The query string deserialized into `T`, which must own its data.
///
/// Fields that are `Option` or have defaults may be absent; anything the struct cannot take is a
/// 400. For a look at a single parameter without deserializing a struct, take
/// `fast_http_core::QueryParams` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Query<T>(pub T);

impl<T: DeserializeOwned> FromParts for Query<T> {
    type Rejection = FormatError;

    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection> {
        serde_urlencoded::from_str(parts.uri.query().unwrap_or(""))
            .map(Self)
            .map_err(FormatError::decode)
    }
}

#[cfg(test)]
mod tests;
