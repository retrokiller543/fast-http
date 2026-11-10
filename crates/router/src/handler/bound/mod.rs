use std::future::Future;

use fast_http_core::{
    ErrorHandler, FromParams, HandlerError, IntoOutcome, PathParams, ReqBody, Response,
};
use http::request::Parts;

use super::{Call, Extract, Handler};

/// Marker for handlers wrapped by [`bind`], keeping them apart from plain functions.
pub struct Bind;

/// A handler function together with the state it takes as its first argument.
///
/// Built by [`bind`]. The state is cloned for every request, so use a cheap-clone handle such as
/// an `Arc`. There is no registry: the state is whatever was passed at registration.
#[derive(Debug, Clone)]
pub struct Bound<S, F> {
    state: S,
    handler: F,
}

/// Binds `state` as the first argument of `handler`, whose remaining arguments are extractors.
///
/// ```ignore
/// async fn show(db: Arc<Db>, Path(id): Path<u64>) -> String { /* ... */ }
///
/// Route::new("/users/{id}")?.get(bind(db, show))
/// ```
pub fn bind<S, F>(state: S, handler: F) -> Bound<S, F> {
    Bound { state, handler }
}

impl<S, F, Fut, R> Handler<(Bind,)> for Bound<S, F>
where
    S: Clone + Send + Sync + 'static,
    F: Fn(S) -> Fut + Send + Sync + 'static,
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

        match (self.handler)(self.state.clone()).await.into_outcome(parts) {
            Ok(response) => response,
            Err(error) => handler.handle(error, parts).await,
        }
    }
}

macro_rules! bound_impl {
    ([$($t:ident $v:ident),*] $l:ident $lv:ident) => {
        impl<S, F, Fut, R, M, $($t,)* $l> Handler<(Bind, M, $($t,)* $l)> for Bound<S, F>
        where
            S: Clone + Send + Sync + 'static,
            F: Fn(S, $($t,)* $l) -> Fut + Send + Sync + 'static,
            Fut: Future<Output = R>,
            R: IntoOutcome,
            $($t: FromParams,)*
            $l: Extract<M>,
            for<'r> F: Call<'r, (S, $($t::At<'r>,)* <$l as Extract<M>>::At<'r>,), Output = R>,
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
                let state = self.state.clone();
                let output = <F as Call<'r, _>>::call(&self.handler, (state, $($v,)* $lv,)).await;

                match output.into_outcome(parts) {
                    Ok(response) => response,
                    Err(error) => handler.handle(error, parts).await,
                }
            }
        }
    };
}

bound_impl!([] T1 v1);
bound_impl!([T1 v1] T2 v2);
bound_impl!([T1 v1, T2 v2] T3 v3);
bound_impl!([T1 v1, T2 v2, T3 v3] T4 v4);
bound_impl!([T1 v1, T2 v2, T3 v3, T4 v4] T5 v5);
bound_impl!([T1 v1, T2 v2, T3 v3, T4 v4, T5 v5] T6 v6);

#[cfg(test)]
mod tests;
