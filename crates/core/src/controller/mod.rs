use http::request::Parts;

use crate::{ErrorHandler, GuardPhase, PathParams, ReqBody, Response};

/// A group of endpoints under one path prefix, matched once and then dispatched on the relative path.
pub trait Controller: Send + Sync + 'static {
    /// Base path shared by every endpoint, without a trailing slash.
    const PREFIX: &'static str;

    /// `Body` when any endpoint needs the buffered body.
    const PHASE: GuardPhase;

    /// Picks the endpoint for `rest`, the path after the prefix, and renders its response.
    ///
    /// Renders 404, 405 and guard-miss itself, so the router never needs the request back.
    fn dispatch<B: ReqBody, H: ErrorHandler>(
        &self,
        rest: &str,
        parts: &Parts,
        params: PathParams<'_>,
        body: B,
        handler: &H,
    ) -> impl Future<Output = Response> + Send;
}

/// Returns the path after `prefix` when it ends on a segment boundary.
pub fn strip_prefix<'a>(prefix: &str, path: &'a str) -> Option<&'a str> {
    let rest = path.strip_prefix(prefix)?;

    if rest.is_empty() || rest.starts_with('/') || prefix.is_empty() {
        return Some(rest);
    }

    None
}

#[cfg(test)]
mod tests;
