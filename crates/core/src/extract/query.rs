use std::convert::Infallible;

use http::request::Parts;

use crate::{FromParams, PathParams};

/// A zero-copy view of the raw query string; nothing is parsed until a method scans it.
///
/// Names and values are compared as sent, without percent-decoding, and a parameter given
/// without `=` has an empty value. It borrows from the request, so it costs nothing to take as a
/// handler argument and is meant for guards that look at one parameter. Use `Query<T>` from
/// `fast-http-format` to deserialize a struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryParams<'r>(&'r str);

impl<'r> QueryParams<'r> {
    /// The view of `parts`' query string; empty when the URI has none.
    pub fn of(parts: &'r Parts) -> Self {
        Self(parts.uri.query().unwrap_or(""))
    }

    /// The query string as sent.
    pub fn raw(&self) -> &'r str {
        self.0
    }

    /// The `(name, value)` pairs in order, skipping empty segments.
    pub fn iter(&self) -> impl Iterator<Item = (&'r str, &'r str)> + use<'r> {
        self.0
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| pair.split_once('=').unwrap_or((pair, "")))
    }

    /// The value of the first parameter called `name`.
    pub fn get(&self, name: &str) -> Option<&'r str> {
        self.iter()
            .find_map(|(key, value)| (key == name).then_some(value))
    }

    /// The values of every parameter called `name`, in order.
    pub fn get_all<'n>(&self, name: &'n str) -> impl Iterator<Item = &'r str> + use<'r, 'n> {
        self.iter()
            .filter_map(move |(key, value)| (key == name).then_some(value))
    }

    /// Whether a parameter called `name` is present, with or without a value.
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Whether the query string has no parameters.
    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
}

impl<'a> FromParams for QueryParams<'a> {
    type Rejection = Infallible;
    type At<'r> = QueryParams<'r>;

    fn from_params<'r>(
        parts: &'r Parts,
        _: &PathParams<'r>,
    ) -> Result<Self::At<'r>, Self::Rejection> {
        Ok(QueryParams::of(parts))
    }
}

#[cfg(test)]
mod tests;
