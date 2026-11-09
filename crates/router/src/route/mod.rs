use fast_http_core::{EndpointSet, ErrorHandler, PathParams, ReqBody, Response};
use http::request::Parts;

use crate::{Pattern, PatternError};

/// A route pattern together with the endpoints that answer it.
#[derive(Debug)]
pub struct Route<S> {
    pattern: Pattern,
    set: S,
}

impl<S: EndpointSet> Route<S> {
    /// Parses `pattern` and pairs it with `set`, a tuple of endpoints tried in order.
    pub fn new(pattern: &'static str, set: S) -> Result<Self, PatternError> {
        Ok(Self {
            pattern: Pattern::parse(pattern)?,
            set,
        })
    }

    /// The parsed pattern.
    pub fn pattern(&self) -> &Pattern {
        &self.pattern
    }

    /// The endpoints.
    pub fn set(&self) -> &S {
        &self.set
    }
}

/// The routes of a router, statically typed. Implemented for tuples of [`Route`]s.
pub trait RouteList: Send + Sync + 'static {
    /// Number of routes.
    const LEN: usize;

    /// Pattern of the route at `index`.
    fn pattern(&self, index: usize) -> &Pattern;

    /// Dispatches to the route at `index`.
    fn call<B: ReqBody, H: ErrorHandler>(
        &self,
        index: usize,
        parts: &Parts,
        params: PathParams<'_>,
        body: B,
        handler: &H,
        options: fast_http_core::DispatchOptions,
    ) -> impl Future<Output = Response> + Send;
}

macro_rules! route_list {
    ($len:expr; $($set:ident : $index:tt),+) => {
        impl<$($set: EndpointSet),+> RouteList for ($(Route<$set>,)+) {
            const LEN: usize = $len;

            fn pattern(&self, index: usize) -> &Pattern {
                match index {
                    $($index => &self.$index.pattern,)+
                    _ => unreachable!("index comes from the router's own table"),
                }
            }

            async fn call<Bd: ReqBody, Eh: ErrorHandler>(
                &self,
                index: usize,
                parts: &Parts,
                params: PathParams<'_>,
                body: Bd,
                handler: &Eh,
                options: fast_http_core::DispatchOptions,
            ) -> Response {
                match index {
                    $($index => {
                        fast_http_core::dispatch_set(
                            &self.$index.set,
                            parts,
                            params,
                            body,
                            handler,
                            options,
                        )
                        .await
                    })+
                    _ => unreachable!("index comes from the router's own table"),
                }
            }
        }
    };
}

route_list!(1; A: 0);
route_list!(2; A: 0, B: 1);
route_list!(3; A: 0, B: 1, C: 2);
route_list!(4; A: 0, B: 1, C: 2, D: 3);
route_list!(5; A: 0, B: 1, C: 2, D: 3, E: 4);
route_list!(6; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5);
route_list!(7; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6);
route_list!(8; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7);
route_list!(9; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8);
route_list!(10; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9);
route_list!(11; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10);
route_list!(12; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10, L: 11);
