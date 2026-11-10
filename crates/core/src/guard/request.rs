use http::request::Parts;

/// Whether the query string has a parameter called `name`, with or without a value.
///
/// Names are compared as they appear on the wire, without percent-decoding.
pub fn query_has(parts: &Parts, name: &str) -> bool {
    query_value(parts, name).is_some()
}

/// Whether the query string has a parameter called `name` whose value is exactly `value`.
///
/// Names and values are compared as they appear on the wire, without percent-decoding, so a
/// parameter given more than once matches when any occurrence does.
pub fn query_eq(parts: &Parts, name: &str, value: &str) -> bool {
    pairs(parts).any(|(key, found)| key == name && found == value)
}

fn query_value<'a>(parts: &'a Parts, name: &str) -> Option<&'a str> {
    pairs(parts).find_map(|(key, value)| (key == name).then_some(value))
}

fn pairs(parts: &Parts) -> impl Iterator<Item = (&str, &str)> {
    parts
        .uri
        .query()
        .into_iter()
        .flat_map(|query| query.split('&'))
        .filter(|pair| !pair.is_empty())
        .map(|pair| pair.split_once('=').unwrap_or((pair, "")))
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
