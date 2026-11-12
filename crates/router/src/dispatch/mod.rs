//! Runtime support for endpoints that `#[controller]` generates when variants share a method and
//! path and some of them are guarded.
//!
//! A generated endpoint extracts each distinct argument type at most once, evaluates the guards
//! as soon as the arguments they use exist, and hands the winning handler the values it already
//! extracted. Nothing here allocates and the endpoint never buffers the request body: every variant that
//! reads it takes the same body type, which is parsed lazily, once.

use fast_http_core::{
    ErrorHandler, FromParamsAt, HandlerError, IntoOutcome, PathParams, Rejection, ReqBody,
    Response, Transparent,
};
use http::{StatusCode, request::Parts};

use crate::ExtractAt;

/// Where a generated endpoint's body extractor gets the request body from.
pub struct BodySource<B>(Option<B>);

impl<B: ReqBody> BodySource<B> {
    /// Wraps `body`.
    pub fn new(body: B) -> Self {
        Self(Some(body))
    }

    /// Hands out the body; it can be read once.
    pub fn take(&mut self) -> Result<B, HandlerError> {
        self.0
            .take()
            .ok_or_else(|| HandlerError::internal("the request body was read twice"))
    }

    /// Gives the body back when nothing has read it, so the request can go to another endpoint.
    pub fn into_unread(self) -> Option<B> {
        self.0
    }
}

/// Whether extracting `T` as the last argument consumes the request body.
pub const fn uses_body<'r, T: ExtractAt<'r, M>, M>() -> bool {
    T::USES_BODY
}

/// Extracts an argument that never reads the body.
pub fn extract_head<'r, T: FromParamsAt<'r>>(
    parts: &'r Parts,
    params: &PathParams<'r>,
) -> Result<T, T::Rejection> {
    T::from_params_at(parts, params)
}

/// Extracts the last argument, which may read the body from `source`.
pub fn extract_last<'r, T: ExtractAt<'r, M>, M, B: ReqBody>(
    parts: &'r Parts,
    params: &PathParams<'r>,
    source: &mut BodySource<B>,
) -> impl Future<Output = Result<T, HandlerError>> + Send {
    T::extract_at(parts, params, source)
}

fn unfilled() -> HandlerError {
    HandlerError::internal("a generated endpoint expected an extracted argument")
}

/// Runs `make` and stores its value unless `slot` already holds one, then lends the value, so an
/// argument type that several variants share is extracted once, and only when a guard or the
/// winning variant needs it.
pub async fn fill<T, E: Into<HandlerError>>(
    slot: &mut Option<T>,
    make: impl Future<Output = Result<T, E>>,
) -> Result<&T, HandlerError> {
    if slot.is_none() {
        *slot = Some(make.await.map_err(Into::into)?);
    }

    slot.as_ref().ok_or_else(unfilled)
}

/// Like [`fill`], but moves the value out for the winning handler.
pub async fn take<T, E: Into<HandlerError>>(
    slot: &mut Option<T>,
    make: impl Future<Output = Result<T, E>>,
) -> Result<T, HandlerError> {
    if slot.is_none() {
        *slot = Some(make.await.map_err(Into::into)?);
    }

    slot.take().ok_or_else(unfilled)
}

/// Renders the handler's return value, routing an error through `handler`.
pub async fn finish<R: IntoOutcome, H: ErrorHandler>(
    output: R,
    parts: &Parts,
    handler: &H,
) -> Response {
    match output.into_outcome(parts) {
        Ok(response) => response,
        Err(error) => handler.handle(error, parts).await,
    }
}

/// Renders an extraction failure through `handler`.
pub async fn fail<H: ErrorHandler>(error: HandlerError, parts: &Parts, handler: &H) -> Response {
    handler.handle(error, parts).await
}

/// The error for a request that no guarded variant accepted.
pub fn guard_miss() -> HandlerError {
    HandlerError::new(Rejection::GuardMiss {
        status: StatusCode::BAD_REQUEST,
    })
}

/// A guard's borrow of an argument, resolved to what the guard sees by [`ViewInner`] or
/// [`ViewSelf`].
///
/// `(&Guarded(arg)).view()` picks [`ViewInner`] when the argument is [`Transparent`], so a guard
/// on `job: Body<Job>` reads `&Job`, and falls back to [`ViewSelf`] otherwise. The choice is made
/// by method resolution at compile time.
pub struct Guarded<'a, T>(pub &'a T);

/// What a guard sees of a [`Transparent`] argument: the value it carries.
pub trait ViewInner<'a> {
    /// The carried value.
    type Out;

    /// Borrows the carried value.
    fn view(&self) -> &'a Self::Out;
}

impl<'a, T: Transparent> ViewInner<'a> for Guarded<'a, T> {
    type Out = T::Inner;

    fn view(&self) -> &'a T::Inner {
        self.0.inner()
    }
}

/// What a guard sees of any other argument: the argument itself.
pub trait ViewSelf<'a> {
    /// The argument.
    type Out;

    /// Borrows the argument.
    fn view(&self) -> &'a Self::Out;
}

impl<'a, T> ViewSelf<'a> for &Guarded<'a, T> {
    type Out = T;

    fn view(&self) -> &'a T {
        self.0
    }
}

/// An empty argument slot of type `T`, so a generated endpoint can name the type it holds.
pub const fn slot<T>() -> Option<T> {
    None
}

#[cfg(test)]
mod tests;
