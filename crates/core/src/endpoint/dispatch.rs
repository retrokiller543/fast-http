use bytes::Bytes;
use http::{StatusCode, request::Parts};
use http_body_util::Full;

use crate::{
    Attempt, DEFAULT_BODY_LIMIT, EndpointSet, ErrorHandler, HandlerError, PathParams, ReqBody,
    Response, Selection, body::buffer,
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

/// Selects and calls an endpoint, buffering the body once if a body guard needs it and rendering rejections.
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
    let selection = set.select(0, parts, None);

    if selection == Selection::NeedBody {
        let bytes = match buffer(body, options.body_limit).await {
            Ok(bytes) => bytes,
            Err(rejection) => return handler.handle(HandlerError::new(rejection), parts).await,
        };
        let selection = set.select(0, parts, Some(&bytes));

        return run(
            set,
            selection,
            parts,
            params,
            Full::<Bytes>::new(bytes.clone()),
            handler,
            options,
            |from| set.select(from, parts, Some(&bytes)),
        )
        .await;
    }

    run(
        set,
        selection,
        parts,
        params,
        body,
        handler,
        options,
        |from| set.select(from, parts, None),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run<S: EndpointSet, B: ReqBody, H: ErrorHandler>(
    set: &S,
    mut selection: Selection,
    parts: &Parts,
    params: PathParams<'_>,
    mut body: B,
    handler: &H,
    options: DispatchOptions,
    reselect: impl Fn(usize) -> Selection,
) -> Response {
    loop {
        match selection {
            Selection::Selected(index) => {
                match set.attempt(index, parts, params, body, handler).await {
                    Attempt::Done(response) => return response,
                    Attempt::Declined(returned) => {
                        body = returned;
                        selection = reselect(index + 1);
                    }
                }
            }
            Selection::Rejected(rejection) => {
                let rejection = rejection.with_guard_miss(options.guard_miss);

                return handler.handle(HandlerError::new(rejection), parts).await;
            }
            Selection::NeedBody => {
                let error = HandlerError::internal("endpoint asked for the body twice");

                return handler.handle(error, parts).await;
            }
        }
    }
}
