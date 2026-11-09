use fast_http_core::{ErrorHandler, Flow, HeadGuard, Next, ReqBody, Request, Response};
use http::request::Parts;

/// Sends the request through `then` when the predicate accepts its head, otherwise through `otherwise`.
///
/// Both arms continue with the same inner pipeline. The predicate is a plain `fn`, so a branch is
/// as cheap to evaluate as an endpoint head guard.
#[derive(Debug, Clone, Copy)]
pub struct Branch<A, B> {
    when: fn(&Parts) -> bool,
    then: A,
    otherwise: B,
}

impl<A, B> Branch<A, B> {
    /// Branches on `when`.
    pub const fn new(when: fn(&Parts) -> bool, then: A, otherwise: B) -> Self {
        Self {
            when,
            then,
            otherwise,
        }
    }

    /// Branches on whether the [`HeadGuard`] `G` matches.
    pub fn on<G: HeadGuard>(then: A, otherwise: B) -> Self {
        Self::new(|parts| G::check(parts).is_some(), then, otherwise)
    }
}

impl<A: Flow, B: Flow> Flow for Branch<A, B> {
    async fn handle<Bd: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        req: Request<Bd>,
        next: N,
        errors: &H,
    ) -> Response {
        let (parts, body) = req.into_parts();
        let taken = (self.when)(&parts);
        let req = Request::from_parts(parts, body);

        if taken {
            return self.then.handle(req, next, errors).await;
        }

        self.otherwise.handle(req, next, errors).await
    }
}

#[cfg(test)]
mod tests;
