use std::{
    any::Any,
    convert::Infallible,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::{Buf, Bytes};
use fast_http_core::{
    BoxError, ErrorHandler, Flow, HandlerError, Next, ReqBody, Request, Response, ResponseBody,
};
use http_body::Body;
use http_body_util::{BodyExt, combinators::UnsyncBoxBody};
use tower_layer::Layer;
use tower_service::Service;

/// Request body seen by tower layers: the incoming body, boxed once at the adapter boundary.
pub type TowerBody = UnsyncBoxBody<Bytes, BodyError>;

/// Error of a [`TowerBody`]: the incoming body's error, boxed.
///
/// A named type rather than [`BoxError`] so the `dyn` lifetime never appears inside the
/// adapter's future, which otherwise fails higher-ranked `Send` checks.
#[derive(Debug)]
pub struct BodyError(pub BoxError);

impl std::fmt::Display for BodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for BodyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

type BoxFuture<'n, T> = Pin<Box<dyn Future<Output = T> + Send + 'n>>;

/// Object-safe [`Next`], so the inner pipeline has one type a tower layer can wrap.
trait ErasedNext<'n>: Send + 'n {
    fn run(self: Box<Self>, req: Request<TowerBody>)
    -> BoxFuture<'n, Result<Response, Infallible>>;
}

impl<'n, N: Next + 'n> ErasedNext<'n> for N {
    fn run(
        self: Box<Self>,
        req: Request<TowerBody>,
    ) -> BoxFuture<'n, Result<Response, Infallible>> {
        Box::pin(async move { Ok((*self).run(req).await) })
    }
}

/// The inner pipeline as a [`tower_service::Service`]; the service a [`TowerFlow`]'s layer wraps.
///
/// Single-use: a second `call` renders 500. Layers that clone or retry their inner service
/// (`Buffer`, `Retry`) therefore do not fit here and belong in front of the server instead.
pub struct NextService<'n> {
    next: Option<Box<dyn ErasedNext<'n>>>,
}

impl<'n> Service<Request<TowerBody>> for NextService<'n> {
    type Response = Response;
    type Error = Infallible;
    type Future = BoxFuture<'n, Result<Response, Infallible>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<TowerBody>) -> Self::Future {
        match self.next.take() {
            Some(next) => next.run(req),
            None => Box::pin(async {
                let mut response = Response::new(ResponseBody::Empty);

                *response.status_mut() = http::StatusCode::INTERNAL_SERVER_ERROR;

                Ok(response)
            }),
        }
    }
}

/// Glue proving a layer's service speaks our request and response types for every inner lifetime.
#[doc(hidden)]
pub trait TowerLayer<'n>: Send + Sync + 'static {
    fn call(
        &self,
        req: Request<TowerBody>,
        next: NextService<'n>,
    ) -> BoxFuture<'n, Result<Response, BoxError>>;
}

impl<'n, L, S, RB> TowerLayer<'n> for L
where
    L: Layer<NextService<'n>, Service = S> + Send + Sync + 'static,
    S: Service<Request<TowerBody>, Response = http::Response<RB>> + Send + 'n,
    S::Future: Send + 'n,
    S::Error: Into<BoxError>,
    RB: Body<Data = Bytes> + Send + 'static,
    RB::Error: Into<BoxError>,
{
    fn call(
        &self,
        req: Request<TowerBody>,
        next: NextService<'n>,
    ) -> BoxFuture<'n, Result<Response, BoxError>> {
        let mut service = self.layer(next);

        Box::pin(async move {
            Ready::new(&mut service).await.map_err(Into::into)?;

            let response = service.call(req).await.map_err(Into::into)?;

            Ok(response.map(into_response_body))
        })
    }
}

/// Waits for `poll_ready`; a named future because a `poll_fn` closure trips higher-ranked `Send` checks.
struct Ready<'a, S, R> {
    service: &'a mut S,
    request: PhantomData<fn(R)>,
}

impl<'a, S, R> Ready<'a, S, R> {
    fn new(service: &'a mut S) -> Self {
        Self {
            service,
            request: PhantomData,
        }
    }
}

impl<S: Service<R>, R> Future for Ready<'_, S, R> {
    type Output = Result<(), S::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.get_mut().service.poll_ready(cx)
    }
}

fn into_response_body<RB>(body: RB) -> ResponseBody
where
    RB: Body<Data = Bytes> + Send + 'static,
    RB::Error: Into<BoxError>,
{
    let mut slot = Some(body);

    match (&mut slot as &mut dyn Any).downcast_mut::<Option<ResponseBody>>() {
        Some(same) => same.take().unwrap_or_default(),
        None => ResponseBody::stream(slot.take().expect("slot was just filled")),
    }
}

/// Runs a [`tower_layer::Layer`] as a [`Flow`].
///
/// The layer wraps a [`NextService`] per request. The request body is boxed into [`TowerBody`]
/// and a foreign response body is boxed into [`ResponseBody::Stream`], so this costs a few
/// allocations per request; native flows cost none. Errors from the layer (a timeout, a load
/// shed) go through the scope's error handler as [`HandlerError::internal`] unless a mapper is set.
#[derive(Debug, Clone, Copy)]
pub struct TowerFlow<L> {
    layer: L,
    map_err: fn(BoxError) -> HandlerError,
}

impl<L> TowerFlow<L> {
    /// Adapts `layer`.
    pub fn new(layer: L) -> Self {
        Self {
            layer,
            map_err: HandlerError::internal,
        }
    }

    /// Converts the layer's errors with `map_err`, e.g. to give a timeout its own status.
    pub fn map_err(mut self, map_err: fn(BoxError) -> HandlerError) -> Self {
        self.map_err = map_err;

        self
    }
}

impl<L> Flow for TowerFlow<L>
where
    L: for<'n> TowerLayer<'n>,
{
    async fn handle<B: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        req: Request<B>,
        next: N,
        errors: &H,
    ) -> Response {
        let (parts, body) = req.into_parts();
        let head = Request::from_parts(parts.clone(), ()).into_parts().0;
        let body = body
            .map_frame(|frame| frame.map_data(|mut data| data.copy_to_bytes(data.remaining())))
            .map_err(|error| BodyError(error.into()))
            .boxed_unsync();
        let next = NextService {
            next: Some(Box::new(next)),
        };

        match self
            .layer
            .call(Request::from_parts(parts, body), next)
            .await
        {
            Ok(response) => response,
            Err(error) => errors.handle((self.map_err)(error), &head).await,
        }
    }
}

#[cfg(test)]
mod tests;
