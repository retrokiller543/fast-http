use bytes::Bytes;
use http::{Method, request::Parts};

use crate::{ErrorHandler, GuardPhase, PathParams, ReqBody, Response};

mod dispatch;
mod set;

pub use dispatch::{DispatchOptions, dispatch_set};
pub use set::{EndpointSet, Selection};

/// One handler variant for a route. Generated or hand-written; nothing here needs macros.
///
/// The path is the router's concern. Variants sharing a path are tried in the order they appear in their set.
pub trait Endpoint: Send + Sync + 'static {
    /// Method this endpoint answers.
    fn method(&self) -> &Method;

    /// Phase of the latest guard this endpoint needs.
    fn phase(&self) -> GuardPhase {
        GuardPhase::Head
    }

    /// Evaluates the head guards.
    fn head_guard(&self, parts: &Parts) -> bool {
        let _ = parts;

        true
    }

    /// Evaluates the body guards against the buffered body; only called when [`Endpoint::phase`] is `Body`.
    fn body_guard(&self, parts: &Parts, body: &Bytes) -> bool {
        let _ = (parts, body);

        true
    }

    /// Extracts arguments, runs the handler and renders its response, routing every error through `handler`.
    fn call<B: ReqBody, H: ErrorHandler>(
        &self,
        parts: &Parts,
        params: PathParams<'_>,
        body: B,
        handler: &H,
    ) -> impl Future<Output = Response> + Send;
}

#[cfg(test)]
mod tests;
