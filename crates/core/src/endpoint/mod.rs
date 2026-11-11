use bytes::Bytes;
use http::{Method, request::Parts};

use crate::{ErrorHandler, GuardPhase, PathParams, ReqBody, Response};

mod attempt;
mod dispatch;
mod either;
mod set;

pub use attempt::Attempt;
pub use dispatch::{DispatchOptions, dispatch_set};
pub use either::{
    EndpointFuture1, EndpointFuture2, EndpointFuture3, EndpointFuture4, EndpointFuture5,
    EndpointFuture6, EndpointFuture7, EndpointFuture8, EndpointFuture9, EndpointFuture10,
    EndpointFuture11, EndpointFuture12, EndpointFuture13, EndpointFuture14, EndpointFuture15,
    EndpointFuture16,
};
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

    /// Whether a guard of [`Endpoint::attempt`] reads the body itself, so that turning the
    /// request down consumes it and yields [`Attempt::Missed`].
    ///
    /// When it does and a later endpoint could take the request, the dispatcher buffers the body
    /// first and replays it to that endpoint.
    fn guards_read_body(&self) -> bool {
        false
    }

    /// Extracts arguments, runs the handler and renders its response, routing every error through `handler`.
    fn call<'r, B: ReqBody, H: ErrorHandler>(
        &self,
        parts: &'r Parts,
        params: PathParams<'r>,
        body: B,
        handler: &H,
    ) -> impl Future<Output = Response> + Send;

    /// Like [`Endpoint::call`], but may decline the request and hand the unread body back, so the
    /// next endpoint of the set gets its turn. The default never declines.
    fn attempt<'r, B: ReqBody, H: ErrorHandler>(
        &self,
        parts: &'r Parts,
        params: PathParams<'r>,
        body: B,
        handler: &H,
    ) -> impl Future<Output = Attempt<B>> + Send {
        async move { Attempt::Done(self.call(parts, params, body, handler).await) }
    }
}

#[cfg(test)]
mod tests;
