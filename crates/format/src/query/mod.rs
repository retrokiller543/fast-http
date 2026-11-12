use fast_http_core::{FromParamsAt, PathParams};
use http::request::Parts;
use serde::de::Deserialize;

use crate::{
    FormatError,
    wrapper::{Wrapper, transparent},
};

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
pub struct Query<T>(pub T);

impl<T> Wrapper for Query<T> {
    type Inner = T;

    fn get(&self) -> &T {
        &self.0
    }

    fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }

    fn unwrap(self) -> T {
        self.0
    }

    fn wrap(value: T) -> Self {
        Self(value)
    }
}

transparent!(value Query [T] [T]);

impl<'r, T: Deserialize<'r> + Send> FromParamsAt<'r> for Query<T> {
    type Rejection = FormatError;

    fn from_params_at(parts: &'r Parts, _: &PathParams<'r>) -> Result<Self, Self::Rejection> {
        let query = parts.uri.query().unwrap_or("");

        serde_urlencoded::from_str(query)
            .map(Self)
            .map_err(|error| {
                if error.to_string().contains("borrowed string") {
                    return FormatError::decode(BorrowedQueryField(error));
                }

                FormatError::decode(error)
            })
    }
}

/// A query value that needs percent-decoding was deserialized into a plain `&str` field, which
/// cannot hold decoded text. The deserializer's error is the source.
#[derive(Debug, thiserror::Error)]
#[error("{0}; a field that borrows from the query must be `Cow<'_, str>` with `#[serde(borrow)]`")]
pub struct BorrowedQueryField(#[source] pub serde_urlencoded::de::Error);

#[cfg(test)]
mod tests;
