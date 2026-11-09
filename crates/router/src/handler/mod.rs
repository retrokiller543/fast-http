use std::{future::Future, marker::PhantomData};

use fast_http_core::{
    Endpoint, ErrorHandler, FromParams, FromRequest, HandlerError, IntoOutcome, PathParams,
    ReqBody, Response,
};
use http::{Method, request::Parts};

/// Marker: the argument is built from the head and path parameters alone.
pub struct ViaParams;

/// Marker: the argument consumes the request body, so it must be last.
pub struct ViaBody;

/// Builds one handler argument; the marker `M` keeps the head-only and body-consuming paths apart.
pub trait Extract<M>: Sized {
    /// Extracts the argument, failing with an already-boxed error.
    fn extract<B: ReqBody>(
        parts: &Parts,
        params: &PathParams<'_>,
        body: B,
    ) -> impl Future<Output = Result<Self, HandlerError>> + Send;
}

impl<T: FromParams> Extract<ViaParams> for T {
    async fn extract<B: ReqBody>(
        parts: &Parts,
        params: &PathParams<'_>,
        body: B,
    ) -> Result<Self, HandlerError> {
        drop(body);

        T::from_params(parts, params).map_err(HandlerError::from)
    }
}

impl<T: FromRequest> Extract<ViaBody> for T {
    async fn extract<B: ReqBody>(
        parts: &Parts,
        _: &PathParams<'_>,
        body: B,
    ) -> Result<Self, HandlerError> {
        T::from_request(parts, body)
            .await
            .map_err(HandlerError::from)
    }
}

/// An async function whose arguments are extractors and whose return value converts into a response.
pub trait Handler<M>: Send + Sync + 'static {
    /// Extracts the arguments, runs the function and renders its result, routing every error through `handler`.
    fn call<B: ReqBody, H: ErrorHandler>(
        &self,
        parts: &Parts,
        params: PathParams<'_>,
        body: B,
        handler: &H,
    ) -> impl Future<Output = Response> + Send;
}

impl<F, Fut, R> Handler<()> for F
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = R> + Send,
    R: IntoOutcome,
{
    async fn call<B: ReqBody, H: ErrorHandler>(
        &self,
        parts: &Parts,
        _: PathParams<'_>,
        body: B,
        handler: &H,
    ) -> Response {
        drop(body);

        match self().await.into_outcome(parts) {
            Ok(response) => response,
            Err(error) => handler.handle(error, parts).await,
        }
    }
}

macro_rules! handler_impl {
    ([$($t:ident $v:ident),*] $l:ident $lv:ident) => {
        impl<F, Fut, R, M, $($t,)* $l> Handler<(M, $($t,)* $l)> for F
        where
            F: Fn($($t,)* $l) -> Fut + Send + Sync + 'static,
            Fut: Future<Output = R> + Send,
            R: IntoOutcome,
            $($t: FromParams + Send,)*
            $l: Extract<M> + Send,
        {
            async fn call<B: ReqBody, H: ErrorHandler>(
                &self,
                parts: &Parts,
                params: PathParams<'_>,
                body: B,
                handler: &H,
            ) -> Response {
                $(
                    let $v = match <$t as FromParams>::from_params(parts, &params) {
                        Ok(value) => value,
                        Err(error) => return handler.handle(HandlerError::from(error), parts).await,
                    };
                )*
                let $lv = match <$l as Extract<M>>::extract(parts, &params, body).await {
                    Ok(value) => value,
                    Err(error) => return handler.handle(error, parts).await,
                };

                match self($($v,)* $lv).await.into_outcome(parts) {
                    Ok(response) => response,
                    Err(error) => handler.handle(error, parts).await,
                }
            }
        }
    };
}

handler_impl!([] T1 v1);
handler_impl!([T1 v1] T2 v2);
handler_impl!([T1 v1, T2 v2] T3 v3);
handler_impl!([T1 v1, T2 v2, T3 v3] T4 v4);
handler_impl!([T1 v1, T2 v2, T3 v3, T4 v4] T5 v5);
handler_impl!([T1 v1, T2 v2, T3 v3, T4 v4, T5 v5] T6 v6);

/// A [`Handler`] bound to a method and an optional head guard, usable as an [`Endpoint`].
pub struct FnEndpoint<F, M> {
    method: Method,
    handler: F,
    guard: Option<fn(&Parts) -> bool>,
    marker: PhantomData<fn() -> M>,
}

impl<F, M> FnEndpoint<F, M> {
    /// Binds `handler` to `method`.
    pub fn new(method: Method, handler: F) -> Self {
        Self {
            method,
            handler,
            guard: None,
            marker: PhantomData,
        }
    }

    /// Only selects this endpoint when `guard` accepts the request head.
    pub fn guard(mut self, guard: fn(&Parts) -> bool) -> Self {
        self.guard = Some(guard);

        self
    }
}

impl<F: Handler<M>, M: 'static> Endpoint for FnEndpoint<F, M> {
    fn method(&self) -> &Method {
        &self.method
    }

    fn head_guard(&self, parts: &Parts) -> bool {
        self.guard.is_none_or(|guard| guard(parts))
    }

    fn call<B: ReqBody, H: ErrorHandler>(
        &self,
        parts: &Parts,
        params: PathParams<'_>,
        body: B,
        handler: &H,
    ) -> impl Future<Output = Response> + Send {
        self.handler.call(parts, params, body, handler)
    }
}

macro_rules! method_fn {
    ($($name:ident => $method:ident),+) => {
        $(
            /// Binds `handler` to this HTTP method.
            pub fn $name<F: Handler<M>, M>(handler: F) -> FnEndpoint<F, M> {
                FnEndpoint::new(Method::$method, handler)
            }
        )+
    };
}

method_fn!(
    get => GET,
    post => POST,
    put => PUT,
    patch => PATCH,
    delete => DELETE
);
