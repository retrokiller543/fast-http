use fast_http_core::{ErrorHandler, Flow, Next, ReqBody, Request, Response};
use http::request::Parts;

/// Edits the request head before the inner pipeline; built by [`map_request`].
#[derive(Debug, Clone, Copy)]
pub struct MapRequest<F>(F);

/// Rewrites the response after the inner pipeline; built by [`map_response`].
#[derive(Debug, Clone, Copy)]
pub struct MapResponse<F>(F);

/// A flow that runs `edit` on the request head, e.g. to insert extensions or normalize headers.
pub const fn map_request<F>(edit: F) -> MapRequest<F>
where
    F: Fn(&mut Parts) + Send + Sync + 'static,
{
    MapRequest(edit)
}

/// A flow that passes every response through `map`, e.g. to add headers.
pub const fn map_response<F>(map: F) -> MapResponse<F>
where
    F: Fn(Response) -> Response + Send + Sync + 'static,
{
    MapResponse(map)
}

impl<F> Flow for MapRequest<F>
where
    F: Fn(&mut Parts) + Send + Sync + 'static,
{
    fn handle<B: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        req: Request<B>,
        next: N,
        _: &H,
    ) -> impl Future<Output = Response> + Send {
        let (mut parts, body) = req.into_parts();

        (self.0)(&mut parts);

        next.run(Request::from_parts(parts, body))
    }
}

impl<F> Flow for MapResponse<F>
where
    F: Fn(Response) -> Response + Send + Sync + 'static,
{
    async fn handle<B: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        req: Request<B>,
        next: N,
        _: &H,
    ) -> Response {
        (self.0)(next.run(req).await)
    }
}

#[cfg(test)]
mod tests;
