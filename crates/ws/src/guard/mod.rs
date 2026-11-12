use fast_http_core::HeadGuard;
use http::{
    HeaderMap, Method,
    header::{CONNECTION, UPGRADE},
    request::Parts,
};

/// Matches requests that ask for a WebSocket upgrade, so a route can serve both a plain `GET`
/// and a WebSocket at the same path.
#[derive(Debug, Clone, Copy)]
pub struct WebSocketRequest;

impl HeadGuard for WebSocketRequest {
    type Output = ();

    fn check(parts: &Parts) -> Option<()> {
        (parts.method == Method::GET
            && has_token(&parts.headers, &CONNECTION, "upgrade")
            && has_token(&parts.headers, &UPGRADE, "websocket"))
        .then_some(())
    }
}

pub(crate) fn has_token(headers: &HeaderMap, name: &http::HeaderName, token: &str) -> bool {
    headers
        .get_all(name)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|candidate| candidate.trim().eq_ignore_ascii_case(token))
}

#[cfg(test)]
mod tests;
