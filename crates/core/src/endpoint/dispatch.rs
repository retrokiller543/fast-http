use bytes::Bytes;
use http::{StatusCode, request::Parts};
use http_body_util::Full;

use crate::{
    Attempt, DEFAULT_BODY_LIMIT, EndpointSet, ErrorHandler, HandlerError, PathParams, Rejection,
    ReqBody, Response, Selection, body::buffer,
};

/// Limits and fallbacks applied while dispatching to an [`EndpointSet`].
#[derive(Debug, Clone, Copy)]
pub struct DispatchOptions {
    /// Largest body buffered for body guards.
    pub body_limit: usize,

    /// Status when the path and method exist but every guard failed.
    pub guard_miss: StatusCode,
}

impl Default for DispatchOptions {
    fn default() -> Self {
        Self {
            body_limit: DEFAULT_BODY_LIMIT,
            guard_miss: StatusCode::BAD_REQUEST,
        }
    }
}

/// Selects and calls an endpoint, rendering rejections.
///
/// The body streams to the endpoint that takes the request. It is buffered, once, only when an
/// endpoint needs the bytes for a guard, or when an endpoint whose guards read the body could miss
/// while a later endpoint might still take the request; from then on every endpoint tried gets
/// its own replay of the bytes.
pub async fn dispatch_set<S, B, H>(
    set: &S,
    parts: &Parts,
    params: PathParams<'_>,
    body: B,
    handler: &H,
    options: DispatchOptions,
) -> Response
where
    S: EndpointSet,
    B: ReqBody,
    H: ErrorHandler,
{
    let (from, body) = match stream(set, parts, params, body, handler, options).await {
        Ok(response) => return response,
        Err(pending) => pending,
    };
    let bytes = match buffer(body, options.body_limit).await {
        Ok(bytes) => bytes,
        Err(rejection) => return handler.handle(HandlerError::new(rejection), parts).await,
    };

    replay(set, from, parts, params, &bytes, handler, options).await
}

/// Tries endpoints on the streaming body; `Err` hands back where to resume once it is buffered.
async fn stream<S: EndpointSet, B: ReqBody, H: ErrorHandler>(
    set: &S,
    parts: &Parts,
    params: PathParams<'_>,
    mut body: B,
    handler: &H,
    options: DispatchOptions,
) -> Result<Response, (usize, B)> {
    let mut from = 0;

    loop {
        let index = match set.select(from, parts, None) {
            Selection::Selected(index) => index,
            Selection::NeedBody => return Err((from, body)),
            Selection::Rejected(rejection) => {
                return Ok(reject(rejection, parts, handler, options).await);
            }
        };

        if set.guards_read_body(index) && later_applies(set, index, parts) {
            return Err((index, body));
        }

        match set.attempt(index, parts, params, body, handler).await {
            Attempt::Done(response) => return Ok(response),
            Attempt::Declined(returned) => {
                body = returned;
                from = index + 1;
            }
            Attempt::Missed => return Ok(missed(parts, handler, options).await),
        }
    }
}

/// Tries endpoints from `from` on, giving each a fresh replay of the buffered body.
async fn replay<S: EndpointSet, H: ErrorHandler>(
    set: &S,
    mut from: usize,
    parts: &Parts,
    params: PathParams<'_>,
    bytes: &Bytes,
    handler: &H,
    options: DispatchOptions,
) -> Response {
    loop {
        let index = match set.select(from, parts, Some(bytes)) {
            Selection::Selected(index) => index,
            Selection::Rejected(rejection) => {
                return reject(rejection, parts, handler, options).await;
            }
            Selection::NeedBody => {
                let error = HandlerError::internal("endpoint asked for the body twice");

                return handler.handle(error, parts).await;
            }
        };
        let body = Full::new(bytes.clone());

        match set.attempt(index, parts, params, body, handler).await {
            Attempt::Done(response) => return response,
            Attempt::Declined(_) | Attempt::Missed => from = index + 1,
        }
    }
}

/// Whether an endpoint after `index` could still take the request, judged by the head alone.
fn later_applies<S: EndpointSet>(set: &S, index: usize, parts: &Parts) -> bool {
    !matches!(set.select(index + 1, parts, None), Selection::Rejected(_))
}

async fn reject<H: ErrorHandler>(
    rejection: Rejection,
    parts: &Parts,
    handler: &H,
    options: DispatchOptions,
) -> Response {
    let rejection = rejection.with_guard_miss(options.guard_miss);

    handler.handle(HandlerError::new(rejection), parts).await
}

async fn missed<H: ErrorHandler>(parts: &Parts, handler: &H, options: DispatchOptions) -> Response {
    let rejection = Rejection::GuardMiss {
        status: options.guard_miss,
    };

    handler.handle(HandlerError::new(rejection), parts).await
}
