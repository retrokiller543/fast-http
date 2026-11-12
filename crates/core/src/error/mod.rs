use std::{any::Any, convert::Infallible, error::Error};

use http::{StatusCode, request::Parts};

use crate::{BoxError, IntoResponse, Response, ResponseBody};

/// An error that knows its status and how to render itself; the body format is entirely up to the implementor.
pub trait ErrorResponse: Error + IntoResponse + Send + Sync + 'static {
    /// Status the error maps to.
    fn status(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    /// Renders with access to the request head, e.g. to negotiate `Accept`; defaults to [`IntoResponse`].
    fn respond(self, parts: &Parts) -> Response
    where
        Self: Sized,
    {
        let _ = parts;

        self.into_response()
    }
}

/// Object-safe view of an [`ErrorResponse`], which is not object-safe itself.
trait Erased: Send + Sync + 'static {
    fn status(&self) -> StatusCode;

    fn error(&self) -> &(dyn Error + Send + Sync + 'static);

    fn as_any(&self) -> &dyn Any;

    fn respond(self: Box<Self>, parts: &Parts) -> Response;
}

impl<E: ErrorResponse> Erased for E {
    fn status(&self) -> StatusCode {
        ErrorResponse::status(self)
    }

    fn error(&self) -> &(dyn Error + Send + Sync + 'static) {
        self
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn respond(self: Box<Self>, parts: &Parts) -> Response {
        ErrorResponse::respond(*self, parts)
    }
}

/// The single boxed error type every failure funnels into, regardless of where it originated.
pub struct HandlerError(Box<dyn Erased>);

impl HandlerError {
    /// Boxes an error that renders itself.
    pub fn new<E: ErrorResponse>(error: E) -> Self {
        Self(Box::new(error))
    }

    /// Wraps a foreign error as an opaque 500 whose detail is never sent to the client.
    pub fn internal(error: impl Into<BoxError>) -> Self {
        Self::new(Internal(error.into()))
    }

    /// Status the error maps to.
    pub fn status(&self) -> StatusCode {
        self.0.status()
    }

    /// The underlying error, for logging.
    pub fn error(&self) -> &(dyn Error + Send + Sync + 'static) {
        self.0.error()
    }

    /// Borrows the original error when it is an `E`.
    pub fn downcast_ref<E: ErrorResponse>(&self) -> Option<&E> {
        self.0.as_any().downcast_ref()
    }

    /// Renders the error with the error's own formatting.
    pub fn respond(self, parts: &Parts) -> Response {
        self.0.respond(parts)
    }
}

impl<E: ErrorResponse> From<E> for HandlerError {
    fn from(error: E) -> Self {
        Self::new(error)
    }
}

impl std::fmt::Debug for HandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("HandlerError").field(&self.error()).finish()
    }
}

impl std::fmt::Display for HandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.error(), f)
    }
}

/// Opaque server error produced by [`HandlerError::internal`].
#[derive(Debug, thiserror::Error)]
#[error("internal server error: {0}")]
pub struct Internal(BoxError);

impl IntoResponse for Internal {
    fn into_response(self) -> Response {
        let mut response = Response::new(ResponseBody::from("internal server error"));

        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;

        response
    }
}

impl ErrorResponse for Internal {}

impl ErrorResponse for Infallible {}

/// Final result of an endpoint call before the error handler runs.
pub type Outcome = Result<Response, HandlerError>;

/// Converts a handler return value into an [`Outcome`], with access to the request head.
pub trait IntoOutcome {
    /// Performs the conversion.
    fn into_outcome(self, parts: &Parts) -> Outcome;
}

impl<T: IntoResponse> IntoOutcome for T {
    fn into_outcome(self, _parts: &Parts) -> Outcome {
        Ok(self.into_response())
    }
}

/// Answers with `T`'s response under another status; `T` may be anything that renders, such as a
/// format wrapper whose encoding can fail.
impl<T: IntoOutcome> IntoOutcome for (StatusCode, T) {
    fn into_outcome(self, parts: &Parts) -> Outcome {
        let (status, inner) = self;
        let mut response = inner.into_outcome(parts)?;

        *response.status_mut() = status;

        Ok(response)
    }
}

impl<T: IntoOutcome, E: Into<HandlerError>> IntoOutcome for Result<T, E> {
    fn into_outcome(self, parts: &Parts) -> Outcome {
        match self {
            Ok(value) => value.into_outcome(parts),
            Err(error) => Err(error.into()),
        }
    }
}

/// Central place where every error becomes a response; register one per scope.
pub trait ErrorHandler: Send + Sync + 'static {
    /// Renders the error; the request head is available for content negotiation.
    fn handle(&self, error: HandlerError, parts: &Parts) -> impl Future<Output = Response> + Send;
}

/// Renders each error with its own [`ErrorResponse::respond`].
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultErrorHandler;

impl ErrorHandler for DefaultErrorHandler {
    async fn handle(&self, error: HandlerError, parts: &Parts) -> Response {
        error.respond(parts)
    }
}

#[cfg(test)]
mod tests;
