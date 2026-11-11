use fast_http_core::{FromParamsAt, PathParams};
use http::request::Parts;
use serde::de::Deserialize;

use crate::FormatError;

/// The query string deserialized into `T`, which may own its data or borrow from the request.
///
/// A type that owns everything works as is. To borrow text, write the field as
/// `#[serde(borrow)] Cow<'a, str>` and the handler argument as `Query<Params<'_>>`: the field points
/// into the request and is copied only when percent-decoding or `+` actually changes it. A plain
/// `&'a str` field cannot hold decoded text, so a value that needs decoding fails with an error
/// that says to use `Cow`; it is a request error and not a panic.
///
/// Borrowing works for handlers of `#[controller]`, `#[routes]` and the route attributes. A plain
/// function registered through `Route::get` cannot name how the lifetime maps to the request, so
/// it takes `QueryParams` instead. Fields that are `Option` or have defaults may be absent; anything
/// the struct cannot take is a 400.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Query<T>(pub T);

impl<'r, T: Deserialize<'r> + Send> FromParamsAt<'r> for Query<T> {
    type Rejection = FormatError;

    fn from_params_at(parts: &'r Parts, _: &PathParams<'r>) -> Result<Self, Self::Rejection> {
        let query = parts.uri.query().unwrap_or("");

        serde_urlencoded::from_str(query)
            .map(Self)
            .map_err(|error| {
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

#[cfg(test)]
mod tests;
