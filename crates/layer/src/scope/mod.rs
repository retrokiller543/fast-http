use fast_http_core::{ErrorHandler, Flow, Next, ReqBody, Request, Response, prefix::strip_prefix};

/// Runs `flow` only for requests whose path lies under `prefix`; every other request skips it.
///
/// The prefix matches on a segment boundary, so `/api` covers `/api` and `/api/users` but not
/// `/apiary`. An empty prefix covers every path.
#[derive(Debug, Clone, Copy)]
pub struct Scope<F> {
    prefix: &'static str,
    flow: F,
}

impl<F> Scope<F> {
    /// Applies `flow` to paths under `prefix`.
    pub const fn new(prefix: &'static str, flow: F) -> Self {
        Self { prefix, flow }
    }
}

impl<F: Flow> Flow for Scope<F> {
    async fn handle<B: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        req: Request<B>,
        next: N,
        errors: &H,
    ) -> Response {
        if strip_prefix(self.prefix, req.uri().path()).is_none() {
            return next.run(req).await;
        }

        self.flow.handle(req, next, errors).await
    }
}

#[cfg(test)]
mod tests;
