use http::request::Parts;

use crate::QueryParams;

/// Whether the query string has a parameter called `name`, with or without a value.
///
/// Names are compared as they appear on the wire, without percent-decoding.
pub fn query_has(parts: &Parts, name: &str) -> bool {
    QueryParams::of(parts).contains(name)
}

/// Whether the query string has a parameter called `name` whose value is exactly `value`.
///
/// Names and values are compared as they appear on the wire, without percent-decoding, so a
/// parameter given more than once matches when any occurrence does.
pub fn query_eq(parts: &Parts, name: &str, value: &str) -> bool {
    QueryParams::of(parts)
        .get_all(name)
        .any(|found| found == value)
}

/// Whether the request has a header called `name`.
pub fn header_has(parts: &Parts, name: &str) -> bool {
    parts.headers.contains_key(name)
}

/// Whether the request has a header called `name` whose value is exactly `value`.
///
/// A repeated header matches when any of its values does.
pub fn header_eq(parts: &Parts, name: &str, value: &str) -> bool {
    parts
        .headers
        .get_all(name)
        .iter()
        .any(|found| found.as_bytes() == value.as_bytes())
}
