use std::convert::Infallible;

use fast_http_core::{FromParams, FromParts, PathParams};
use http::request::Parts;
use serde::de::{Deserialize, DeserializeOwned};

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

/// The raw query string of the request, deserialized on demand into a type that may borrow from it.
///
/// Where [`Query`] must own what it reads, `deserialize` accepts any `T: Deserialize<'r>`. Borrow
/// text as `#[serde(borrow)] Cow<'r, str>`: the field points into the request and is copied only
/// when percent-decoding or `+` actually changes it. A plain `&'r str` field cannot hold decoded
/// text, so a value that needs decoding fails with an error that says to use `Cow`, which is a
/// request error and not a panic. Types that own everything work as well, and no work happens
/// unless the handler calls `deserialize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawQuery<'r>(&'r str);

impl<'r> RawQuery<'r> {
    /// The query string as sent, without the leading `?`.
    pub fn as_str(&self) -> &'r str {
        self.0
    }

    /// Deserializes the query string into `T`.
    pub fn deserialize<T: Deserialize<'r>>(&self) -> Result<T, FormatError> {
        serde_urlencoded::from_str(self.0).map_err(|error| {
            let message = error.to_string();

            if message.contains("borrowed string") {
                return FormatError::decode(format!(
                    "{message}; a field that borrows from the query must be `Cow<'_, str>` with `#[serde(borrow)]`"
                ));
            }

            FormatError::decode(error)
        })
    }
}

impl FromParams for RawQuery<'_> {
    type Rejection = Infallible;
    type At<'r> = RawQuery<'r>;

    fn from_params<'r>(
        parts: &'r Parts,
        _: &PathParams<'r>,
    ) -> Result<Self::At<'r>, Self::Rejection> {
        Ok(RawQuery(parts.uri.query().unwrap_or("")))
    }
}

#[cfg(test)]
mod tests;
