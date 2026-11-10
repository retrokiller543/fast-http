use std::{future::poll_fn, pin::pin};

use fast_http_core::{ErrorHandler, Flow, Next, ReqBody, Request, Response, ResponseBody};

use crate::head::head_only;

/// Runs `primary` as usual and mirrors the request head through `shadow`, ignoring its result.
///
/// The shadow gets a copy of the head with no body and no extensions, and its continuation
/// returns an empty response at once, so a shadow flow can audit or log but never reaches the
/// endpoint. Both flows are polled together. The shadow is best effort: it is dropped when the
/// primary finishes, even if it has not, and a shadow rejection never affects the primary.
#[derive(Debug, Clone, Copy)]
pub struct Tee<P, S> {
    primary: P,
    shadow: S,
}

impl<P, S> Tee<P, S> {
    /// Mirrors requests of `primary` through `shadow`.
    pub const fn new(primary: P, shadow: S) -> Self {
        Self { primary, shadow }
    }
}

struct Discard;

impl Next for Discard {
    async fn run<B: ReqBody>(self, req: Request<B>) -> Response {
        drop(req);

        Response::new(ResponseBody::Empty)
    }
}

impl<P: Flow, S: Flow> Flow for Tee<P, S> {
    async fn handle<B: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        req: Request<B>,
        next: N,
        errors: &H,
    ) -> Response {
        let (parts, body) = req.into_parts();
        let copy = head_only(&parts);
        let mut primary = pin!(
            self.primary
                .handle(Request::from_parts(parts, body), next, errors)
        );
        let mut shadow = pin!(self.shadow.handle(copy, Discard, errors));
        let mut shadow_done = false;

        poll_fn(|cx| {
            if !shadow_done {
                shadow_done = shadow.as_mut().poll(cx).is_ready();
            }

            primary.as_mut().poll(cx)
        })
        .await
    }
}

#[cfg(test)]
mod tests;
