use std::sync::{Mutex, PoisonError};

use fast_http_core::{ErrorHandler, Flow, Next, ReqBody, Request, Response};
use http::request::Parts;

use crate::head::head_only;

/// Tries `primary`; when it rejects the request without continuing, runs `secondary` instead.
///
/// The usual use is alternative credentials: `Fallback::new(BasicAuth, BearerAuth)` admits a
/// request that either flow accepts. The primary counts as having rejected when it never called
/// its continuation and its response satisfies `when`, by default any 4xx status.
///
/// The primary sees a copy of the request head with no body and no extensions, so it can inspect
/// headers and the URI but cannot consume the body. If it continues, the original body and
/// extensions are restored, together with the extensions the primary added. The secondary gets
/// the original request untouched.
#[derive(Debug, Clone, Copy)]
pub struct Fallback<A, B> {
    primary: A,
    secondary: B,
    when: fn(&Response) -> bool,
}

impl<A, B> Fallback<A, B> {
    /// Falls back to `secondary` when `primary` answers with a client error without continuing.
    pub const fn new(primary: A, secondary: B) -> Self {
        Self {
            primary,
            secondary,
            when: is_rejection,
        }
    }

    /// Falls back when `when` accepts the primary's response, instead of on any 4xx status.
    pub const fn when(mut self, when: fn(&Response) -> bool) -> Self {
        self.when = when;

        self
    }
}

fn is_rejection(response: &Response) -> bool {
    response.status().is_client_error()
}

struct Held<N, Bd> {
    next: N,
    parts: Parts,
    body: Bd,
}

type Cell<N, Bd> = Mutex<Option<Held<N, Bd>>>;

fn take<N, Bd>(cell: &Cell<N, Bd>) -> Option<Held<N, Bd>> {
    cell.lock().unwrap_or_else(PoisonError::into_inner).take()
}

struct Forward<'a, N, Bd> {
    cell: &'a Cell<N, Bd>,
}

impl<N: Next, Bd: ReqBody> Next for Forward<'_, N, Bd> {
    async fn run<B: ReqBody>(self, req: Request<B>) -> Response {
        let held = take(self.cell).expect("a continuation is consumed by running it");
        let (mut head, _) = req.into_parts();

        head.extensions.extend(held.parts.extensions);

        held.next.run(Request::from_parts(head, held.body)).await
    }
}

impl<A: Flow, B: Flow> Flow for Fallback<A, B> {
    async fn handle<Bd: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        req: Request<Bd>,
        next: N,
        errors: &H,
    ) -> Response {
        let (parts, body) = req.into_parts();
        let probe = head_only(&parts);
        let cell = Mutex::new(Some(Held { next, parts, body }));
        let response = self
            .primary
            .handle(probe, Forward { cell: &cell }, errors)
            .await;

        let Some(held) = take(&cell) else {
            return response;
        };

        if !(self.when)(&response) {
            return response;
        }

        self.secondary
            .handle(
                Request::from_parts(held.parts, held.body),
                held.next,
                errors,
            )
            .await
    }
}

#[cfg(test)]
mod tests;
