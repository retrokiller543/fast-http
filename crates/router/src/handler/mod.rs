use std::{future::Future, marker::PhantomData};

mod bound;

pub use bound::{Bind, Bound, bind};

use crate::dispatch::BodySource;

use bytes::Bytes;
use fast_http_core::{
    Endpoint, ErrorHandler, FromParams, FromParamsAt, FromRequest, GuardPhase, HandlerError,
    IntoOutcome, PathParams, ReqBody, Response,
};
use http::{Method, request::Parts};

/// Marker: the argument is built from the head and path parameters alone.
pub struct ViaParams;

/// Marker: the argument consumes the request body, so it must be last.
pub struct ViaBody;

/// Builds one handler argument; the marker `M` keeps the head-only and body-consuming paths apart.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as a handler argument",
    label = "this is not an extractor, or one of its own requirements is not met",
    note = "an argument implements `FromParams` (built from the request head and path) or `FromRequest` (consumes the body, and must be the last argument)",
    note = "for an extractor such as `Body<T>`, check its requirements too, for example that `T` implements `Deserialize` for an enabled format"
)]
pub trait Extract<M> {
    /// Value produced for a request borrowed for `'r`.
    type At<'r>: Send;

    /// Whether extraction consumes the request body.
    const USES_BODY: bool;

    /// Extracts the argument, failing with an already-boxed error.
    fn extract<'r, B: ReqBody>(
        parts: &'r Parts,
        params: &PathParams<'r>,
        body: B,
    ) -> impl Future<Output = Result<Self::At<'r>, HandlerError>> + Send;

    /// Extracts the argument, taking the body from `source` only if it needs one.
    fn extract_from<'r, B: ReqBody>(
        parts: &'r Parts,
        params: &PathParams<'r>,
        source: &mut BodySource<B>,
    ) -> impl Future<Output = Result<Self::At<'r>, HandlerError>> + Send;
}

impl<T: FromParams> Extract<ViaParams> for T {
    type At<'r> = T::At<'r>;

    const USES_BODY: bool = false;

    async fn extract<'r, B: ReqBody>(
        parts: &'r Parts,
        params: &PathParams<'r>,
        body: B,
    ) -> Result<Self::At<'r>, HandlerError> {
        drop(body);

        T::from_params(parts, params).map_err(HandlerError::from)
    }

    async fn extract_from<'r, B: ReqBody>(
        parts: &'r Parts,
        params: &PathParams<'r>,
        _: &mut BodySource<B>,
    ) -> Result<Self::At<'r>, HandlerError> {
        T::from_params(parts, params).map_err(HandlerError::from)
    }
}

impl<T: FromRequest + Send> Extract<ViaBody> for T {
    type At<'r> = T;

    const USES_BODY: bool = true;

    async fn extract<'r, B: ReqBody>(
        parts: &'r Parts,
        _: &PathParams<'r>,
        body: B,
    ) -> Result<Self, HandlerError> {
        T::from_request(parts, body)
            .await
            .map_err(HandlerError::from)
    }

    async fn extract_from<'r, B: ReqBody>(
        parts: &'r Parts,
        _: &PathParams<'r>,
        source: &mut BodySource<B>,
    ) -> Result<Self, HandlerError> {
        let body = source.take()?;

        T::from_request(parts, body)
            .await
            .map_err(HandlerError::from)
    }
}

/// Builds one argument of a generated endpoint, as the type the handler wrote.
///
/// Unlike [`Extract`], `Self` is the value itself, so a struct that borrows from the request, such
/// as `Query<Params<'_>>`, works without declaring how its lifetime maps to the request's. Plain
/// functions registered through [`Handler`] cannot use this and keep to [`Extract`].
pub trait ExtractAt<'r, M>: Sized + Send {
    /// Whether extraction consumes the request body.
    const USES_BODY: bool;

    /// Extracts the argument, taking the body from `source` only if it needs one.
    fn extract_at<B: ReqBody>(
        parts: &'r Parts,
        params: &PathParams<'r>,
        source: &mut BodySource<B>,
    ) -> impl Future<Output = Result<Self, HandlerError>> + Send;
}

impl<'r, T: FromParamsAt<'r>> ExtractAt<'r, ViaParams> for T {
    const USES_BODY: bool = false;

    async fn extract_at<B: ReqBody>(
        parts: &'r Parts,
        params: &PathParams<'r>,
        _: &mut BodySource<B>,
    ) -> Result<Self, HandlerError> {
        T::from_params_at(parts, params).map_err(HandlerError::from)
    }
}

impl<'r, T: FromRequest + Send> ExtractAt<'r, ViaBody> for T {
    const USES_BODY: bool = true;

    async fn extract_at<B: ReqBody>(
        parts: &'r Parts,
        _: &PathParams<'r>,
        source: &mut BodySource<B>,
    ) -> Result<Self, HandlerError> {
        let body = source.take()?;

        T::from_request(parts, body)
            .await
            .map_err(HandlerError::from)
    }
}

/// Calls a function with arguments borrowed for `'r`, naming the returned future.
///
/// Implemented for every `Fn` of up to seven arguments. [`Handler`] requires it for all `'r`, which
/// is what lets plain `async fn`s take borrowed extractors such as `&Parts` or `Path<&str>`.
pub trait Call<'r, Args>: Send + Sync {
    /// Value the future resolves to.
    type Output;
    /// Future returned by the function.
    type Future: Future<Output = Self::Output> + Send;

    /// Calls the function with `args`.
    fn call(&self, args: Args) -> Self::Future;
}

macro_rules! call_impl {
    ($($t:ident $v:ident),+) => {
        impl<'r, F, Fut, $($t),+> Call<'r, ($($t,)+)> for F
        where
            F: Fn($($t),+) -> Fut + Send + Sync,
            Fut: Future + Send,
        {
            type Output = Fut::Output;
            type Future = Fut;

            fn call(&self, ($($v,)+): ($($t,)+)) -> Fut {
                self($($v),+)
            }
        }
    };
}

call_impl!(T1 v1);
call_impl!(T1 v1, T2 v2);
call_impl!(T1 v1, T2 v2, T3 v3);
call_impl!(T1 v1, T2 v2, T3 v3, T4 v4);
call_impl!(T1 v1, T2 v2, T3 v3, T4 v4, T5 v5);
call_impl!(T1 v1, T2 v2, T3 v3, T4 v4, T5 v5, T6 v6);
call_impl!(T1 v1, T2 v2, T3 v3, T4 v4, T5 v5, T6 v6, T7 v7);

/// An async function whose arguments are extractors and whose return value converts into a response.
pub trait Handler<M>: Send + Sync + 'static {
    /// Extracts the arguments, runs the function and renders its result, routing every error through `handler`.
    fn call<'r, B: ReqBody, H: ErrorHandler>(
        &self,
        parts: &'r Parts,
        params: PathParams<'r>,
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
    async fn call<'r, B: ReqBody, H: ErrorHandler>(
        &self,
        parts: &'r Parts,
        _: PathParams<'r>,
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
        impl<F, Fut, M, $($t,)* $l> Handler<(M, $($t,)* $l)> for F
        where
            F: Fn($($t,)* $l) -> Fut + Send + Sync + 'static,
            $($t: FromParams,)*
            $l: Extract<M>,
            for<'r> F: Call<'r, ($($t::At<'r>,)* <$l as Extract<M>>::At<'r>,)>,
            for<'r> <F as Call<'r, ($($t::At<'r>,)* <$l as Extract<M>>::At<'r>,)>>::Output: IntoOutcome,
        {
            async fn call<'r, B: ReqBody, H: ErrorHandler>(
                &self,
                parts: &'r Parts,
                params: PathParams<'r>,
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
                let output = <F as Call<'r, _>>::call(self, ($($v,)* $lv,)).await;

                match output.into_outcome(parts) {
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

/// A [`Handler`] bound to a method and optional head and body guards, usable as an [`Endpoint`].
pub struct FnEndpoint<F, M> {
    method: Method,
    handler: F,
    guard: Option<fn(&Parts) -> bool>,
    body_guard: Option<fn(&Parts, &Bytes) -> bool>,
    marker: PhantomData<fn() -> M>,
}

impl<F, M> FnEndpoint<F, M> {
    /// Binds `handler` to `method`.
    pub fn new(method: Method, handler: F) -> Self {
        Self {
            method,
            handler,
            guard: None,
            body_guard: None,
            marker: PhantomData,
        }
    }

    /// Only selects this endpoint when `guard` accepts the request head.
    pub fn guard(mut self, guard: fn(&Parts) -> bool) -> Self {
        self.guard = Some(guard);

        self
    }

    /// Only selects this endpoint when `guard` accepts the request head and the buffered body.
    ///
    /// The server buffers the body once, up to the dispatch body limit, and hands the handler
    /// the same bytes as its body. The head guard runs first and a request it rejects never
    /// causes buffering for this endpoint.
    pub fn body_guard(mut self, guard: fn(&Parts, &Bytes) -> bool) -> Self {
        self.body_guard = Some(guard);

        self
    }
}

impl<F: Handler<M>, M: 'static> Endpoint for FnEndpoint<F, M> {
    fn method(&self) -> &Method {
        &self.method
    }

    fn phase(&self) -> GuardPhase {
        if self.body_guard.is_some() {
            return GuardPhase::Body;
        }

        GuardPhase::Head
    }

    fn head_guard(&self, parts: &Parts) -> bool {
        self.guard.is_none_or(|guard| guard(parts))
    }

    fn body_guard(&self, parts: &Parts, body: &Bytes) -> bool {
        self.body_guard.is_none_or(|guard| guard(parts, body))
    }

    fn call<'r, B: ReqBody, H: ErrorHandler>(
        &self,
        parts: &'r Parts,
        params: PathParams<'r>,
        body: B,
        handler: &H,
    ) -> impl Future<Output = Response> + Send {
        self.handler.call(parts, params, body, handler)
    }
}

#[cfg(test)]
mod tests;

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
