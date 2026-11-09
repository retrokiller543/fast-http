use crate::{ErrorHandler, HandlerError, ReqBody, Request, Response};

/// The rest of the pipeline after a [`Flow`]: inner flows, then routing and the endpoint.
///
/// Passed by value and statically typed, so calling it never boxes. Generic over the body so a
/// flow may swap the body type (decompression, limits) before passing the request on.
pub trait Next: Send {
    /// Runs the rest of the pipeline.
    fn run<B: ReqBody>(self, req: Request<B>) -> impl Future<Output = Response> + Send;
}

/// Middleware: sees the request before the inner pipeline and the response after it.
///
/// Failures go through `errors`, the scope's central [`ErrorHandler`], so a rejected request
/// renders exactly like an extractor or handler error. [`reject`] does this in one call.
pub trait Flow: Send + Sync + 'static {
    /// `true` only when the flow forwards the request untouched, letting routers keep the
    /// borrowed fast path instead of handing the request over by value.
    const PASSTHROUGH: bool = false;

    /// Handles the request, usually by calling `next` once.
    fn handle<B: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        req: Request<B>,
        next: N,
        errors: &H,
    ) -> impl Future<Output = Response> + Send;
}

/// Renders `error` through `errors`, discarding the request body.
pub async fn reject<B: ReqBody, H: ErrorHandler>(
    errors: &H,
    req: Request<B>,
    error: impl Into<HandlerError>,
) -> Response {
    let (parts, body) = req.into_parts();

    drop(body);

    errors.handle(error.into(), &parts).await
}

/// The flow that does nothing; the default for every scope.
#[derive(Debug, Default, Clone, Copy)]
pub struct Identity;

impl Flow for Identity {
    const PASSTHROUGH: bool = true;

    fn handle<B: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        req: Request<B>,
        next: N,
        _: &H,
    ) -> impl Future<Output = Response> + Send {
        next.run(req)
    }
}

/// `Outer` wrapped around `Inner`: `Outer` sees the request first and the response last.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stack<Inner, Outer> {
    inner: Inner,
    outer: Outer,
}

impl<Inner, Outer> Stack<Inner, Outer> {
    /// Wraps `outer` around `inner`.
    pub const fn new(inner: Inner, outer: Outer) -> Self {
        Self { inner, outer }
    }

    /// The inner flow.
    pub const fn inner(&self) -> &Inner {
        &self.inner
    }

    /// The outer flow.
    pub const fn outer(&self) -> &Outer {
        &self.outer
    }
}

impl<Inner: Flow, Outer: Flow> Flow for Stack<Inner, Outer> {
    const PASSTHROUGH: bool = Inner::PASSTHROUGH && Outer::PASSTHROUGH;

    fn handle<B: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        req: Request<B>,
        next: N,
        errors: &H,
    ) -> impl Future<Output = Response> + Send {
        let next = Then {
            flow: &self.inner,
            next,
            errors,
        };

        self.outer.handle(req, next, errors)
    }
}

/// A [`Next`] that runs `flow` before `next`; how [`Stack`] chains its halves.
pub struct Then<'f, 'h, F, N, H> {
    flow: &'f F,
    next: N,
    errors: &'h H,
}

impl<'f, 'h, F, N, H> Then<'f, 'h, F, N, H> {
    /// Runs `flow`, then `next`, rendering errors through `errors`.
    pub const fn new(flow: &'f F, next: N, errors: &'h H) -> Self {
        Self { flow, next, errors }
    }
}

impl<F: Flow, N: Next, H: ErrorHandler> Next for Then<'_, '_, F, N, H> {
    fn run<B: ReqBody>(self, req: Request<B>) -> impl Future<Output = Response> + Send {
        self.flow.handle(req, self.next, self.errors)
    }
}

/// Runs `flow` with `next` and `errors`; the entry point routers use for every scope.
pub fn run<F: Flow, B: ReqBody, N: Next, H: ErrorHandler>(
    flow: &F,
    req: Request<B>,
    next: N,
    errors: &H,
) -> impl Future<Output = Response> + Send {
    flow.handle(req, next, errors)
}

#[cfg(test)]
mod tests;
