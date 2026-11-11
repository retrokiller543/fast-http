//! Runtime support for endpoints that `#[controller]` generates when variants share a method and
//! path and some of them are guarded.
//!
//! A generated endpoint extracts each distinct argument type at most once, evaluates the guards
//! as soon as the arguments they use exist, and hands the winning handler the values it already
//! extracted. Nothing here allocates; the request body is buffered only when two variants need
//! different body types.

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use bytes::{Buf, Bytes};
use fast_http_core::{
    BoxError, DEFAULT_BODY_LIMIT, ErrorHandler, FromParams, HandlerError, IntoOutcome, PathParams,
    Rejection, ReqBody, Response, buffer,
};
use http::{StatusCode, request::Parts};
use http_body::{Body, Frame, SizeHint};
use pin_project_lite::pin_project;

use crate::Extract;

/// Where a generated endpoint's body extractor gets the request body from.
///
/// With one body type in play the stream is handed over untouched. With several, the first
/// extraction buffers it once and every extraction gets a cheap clone of the bytes.
pub struct BodySource<B> {
    state: State<B>,
    replay: bool,
}

enum State<B> {
    Fresh(B),
    Buffered(Bytes),
    Taken,
}

impl<B: ReqBody> BodySource<B> {
    /// Wraps `body`; `replay` buffers it on first use so that more than one extractor can read it.
    pub fn new(body: B, replay: bool) -> Self {
        Self {
            state: State::Fresh(body),
            replay,
        }
    }

    /// Hands out the body for one extraction.
    pub async fn take(&mut self) -> Result<Replay<B>, Rejection> {
        match std::mem::replace(&mut self.state, State::Taken) {
            State::Fresh(body) if self.replay => {
                let bytes = buffer(body, DEFAULT_BODY_LIMIT).await?;

                self.state = State::Buffered(bytes.clone());

                Ok(Replay::buffered(bytes))
            }
            State::Fresh(body) => Ok(Replay::Stream { body }),
            State::Buffered(bytes) => {
                self.state = State::Buffered(bytes.clone());

                Ok(Replay::buffered(bytes))
            }
            State::Taken => Ok(Replay::buffered(Bytes::new())),
        }
    }
}

pin_project! {
    /// A request body that is either the original stream or buffered bytes.
    #[project = ReplayProj]
    pub enum Replay<B> {
        /// The untouched request body.
        Stream {
            #[pin]
            body: B,
        },
        /// Bytes that were buffered so several extractors can read them.
        Buffered {
            data: Option<Bytes>,
        },
    }
}

impl<B> Replay<B> {
    fn buffered(bytes: Bytes) -> Self {
        Self::Buffered { data: Some(bytes) }
    }
}

impl<B: ReqBody> Body for Replay<B> {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, BoxError>>> {
        match self.project() {
            ReplayProj::Stream { body } => body.poll_frame(cx).map(|frame| {
                frame.map(|result| {
                    result
                        .map(|frame| {
                            frame.map_data(|mut data| {
                                let length = data.remaining();

                                data.copy_to_bytes(length)
                            })
                        })
                        .map_err(Into::into)
                })
            }),
            ReplayProj::Buffered { data } => Poll::Ready(
                data.take()
                    .filter(|bytes| !bytes.is_empty())
                    .map(|bytes| Ok(Frame::data(bytes))),
            ),
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            Self::Stream { body } => body.is_end_stream(),
            Self::Buffered { data } => data.as_ref().is_none_or(Bytes::is_empty),
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            Self::Stream { body } => {
                let hint = body.size_hint();
                let mut mapped = SizeHint::new();

                mapped.set_lower(hint.lower());

                if let Some(upper) = hint.upper() {
                    mapped.set_upper(upper);
                }

                mapped
            }
            Self::Buffered { data } => {
                SizeHint::with_exact(data.as_ref().map_or(0, Bytes::len) as u64)
            }
        }
    }
}

/// Whether extracting `T` as the last argument consumes the request body.
pub const fn uses_body<T: Extract<M>, M>() -> bool {
    T::USES_BODY
}

/// Extracts an argument that never reads the body.
pub fn extract_head<'r, T: FromParams>(
    parts: &'r Parts,
    params: &PathParams<'r>,
) -> Result<T::At<'r>, T::Rejection> {
    T::from_params(parts, params)
}

/// Extracts the last argument, which may read the body from `source`.
pub fn extract_last<'r, T: Extract<M>, M, B: ReqBody>(
    parts: &'r Parts,
    params: &PathParams<'r>,
    source: &mut BodySource<B>,
) -> impl Future<Output = Result<T::At<'r>, HandlerError>> + Send {
    T::extract_from(parts, params, source)
}

/// Runs `make` and stores its value unless `slot` already holds one, so an argument type that
/// several variants share is extracted once, and only when a variant needs it.
pub async fn fill<T, E: Into<HandlerError>>(
    slot: &mut Option<T>,
    make: impl Future<Output = Result<T, E>>,
) -> Result<(), HandlerError> {
    if slot.is_none() {
        *slot = Some(make.await.map_err(Into::into)?);
    }

    Ok(())
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

/// Renders the response for a request that no guarded variant accepted.
pub async fn guard_miss<H: ErrorHandler>(parts: &Parts, handler: &H) -> Response {
    let rejection = Rejection::GuardMiss {
        status: StatusCode::BAD_REQUEST,
    };

    handler.handle(HandlerError::new(rejection), parts).await
}

/// Renders the error for a slot a generated endpoint expected to be filled and was not.
pub async fn unfilled<H: ErrorHandler>(parts: &Parts, handler: &H) -> Response {
    let error =
        HandlerError::internal("a generated endpoint used an argument before extracting it");

    handler.handle(error, parts).await
}

#[cfg(test)]
mod tests;
