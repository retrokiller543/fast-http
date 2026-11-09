use fast_http_core::{
    DispatchOptions, EndpointSet, ErrorHandler, Flow, HandlerError, Identity, Next, PathParams,
    Rejection, ReqBody, Request, Response, Stack, flow,
};
use http::request::Parts;

use crate::{ParamBuf, Pattern, PatternError};

/// A route pattern together with the endpoints that answer it and the flow wrapped around them.
#[derive(Debug)]
pub struct Route<S, F = Identity> {
    pattern: Pattern,
    set: S,
    flow: F,
}

impl<S: EndpointSet> Route<S> {
    /// Parses `pattern` and pairs it with `set`, a tuple of endpoints tried in order.
    pub fn new(pattern: &'static str, set: S) -> Result<Self, PatternError> {
        Ok(Self {
            pattern: Pattern::parse(pattern)?,
            set,
            flow: Identity,
        })
    }
}

impl<S, F> Route<S, F> {
    /// Wraps `flow` around this route; it runs after matching, and the last one added runs first.
    ///
    /// The route re-matches its pattern after the flows, so a flow that rewrites the path to one
    /// the route no longer matches yields 404.
    pub fn layer<G: Flow>(self, flow: G) -> Route<S, Stack<F, G>> {
        Route {
            pattern: self.pattern,
            set: self.set,
            flow: Stack::new(self.flow, flow),
        }
    }

    /// The parsed pattern.
    pub fn pattern(&self) -> &Pattern {
        &self.pattern
    }

    /// The endpoints.
    pub fn set(&self) -> &S {
        &self.set
    }

    /// The flow wrapped around the endpoints.
    pub fn flow(&self) -> &F {
        &self.flow
    }
}

/// The inside of a layered route: re-matches the possibly rewritten path, then dispatches.
struct RouteNext<'a, S, H> {
    pattern: &'a Pattern,
    set: &'a S,
    handler: &'a H,
    options: DispatchOptions,
}

impl<S: EndpointSet, H: ErrorHandler> Next for RouteNext<'_, S, H> {
    async fn run<B: ReqBody>(self, req: Request<B>) -> Response {
        let (parts, body) = req.into_parts();
        let mut buf = ParamBuf::new();

        if !self.pattern.matches(parts.uri.path(), &mut buf) {
            return self
                .handler
                .handle(HandlerError::new(Rejection::NotFound), &parts)
                .await;
        }

        let params = PathParams::new(self.pattern.names(), buf.as_slice());

        fast_http_core::dispatch_set(self.set, &parts, params, body, self.handler, self.options)
            .await
    }
}

/// The routes of a router, statically typed. Implemented for tuples of [`Route`]s.
pub trait RouteList: Send + Sync + 'static {
    /// Number of routes.
    const LEN: usize;

    /// Pattern of the route at `index`.
    fn pattern(&self, index: usize) -> &Pattern;

    /// Whether the route at `index` has no flows, so [`RouteList::call`] applies.
    fn passthrough(&self, index: usize) -> bool;

    /// Dispatches to the route at `index`, which must be [`passthrough`](RouteList::passthrough).
    fn call<B: ReqBody, H: ErrorHandler>(
        &self,
        index: usize,
        parts: &Parts,
        params: PathParams<'_>,
        body: B,
        handler: &H,
        options: DispatchOptions,
    ) -> impl Future<Output = Response> + Send;

    /// Runs the flows of the route at `index` around its endpoints.
    fn call_layered<B: ReqBody, H: ErrorHandler>(
        &self,
        index: usize,
        req: Request<B>,
        handler: &H,
        options: DispatchOptions,
    ) -> impl Future<Output = Response> + Send;
}

macro_rules! route_list {
    ($len:expr; $($set:ident $flow:ident : $index:tt),+) => {
        impl<$($set: EndpointSet, $flow: Flow),+> RouteList for ($(Route<$set, $flow>,)+) {
            const LEN: usize = $len;

            fn pattern(&self, index: usize) -> &Pattern {
                match index {
                    $($index => &self.$index.pattern,)+
                    _ => unreachable!("index comes from the router's own table"),
                }
            }

            fn passthrough(&self, index: usize) -> bool {
                match index {
                    $($index => $flow::PASSTHROUGH,)+
                    _ => unreachable!("index comes from the router's own table"),
                }
            }

            async fn call_layered<Bd: ReqBody, Eh: ErrorHandler>(
                &self,
                index: usize,
                req: Request<Bd>,
                handler: &Eh,
                options: DispatchOptions,
            ) -> Response {
                match index {
                    $($index => {
                        let route = &self.$index;
                        let next = RouteNext {
                            pattern: &route.pattern,
                            set: &route.set,
                            handler,
                            options,
                        };

                        flow::run(&route.flow, req, next, handler).await
                    })+
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
                options: DispatchOptions,
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

route_list!(1; SA FA: 0);
route_list!(2; SA FA: 0, SB FB: 1);
route_list!(3; SA FA: 0, SB FB: 1, SC FC: 2);
route_list!(4; SA FA: 0, SB FB: 1, SC FC: 2, SD FD: 3);
route_list!(5; SA FA: 0, SB FB: 1, SC FC: 2, SD FD: 3, SE FE: 4);
route_list!(6; SA FA: 0, SB FB: 1, SC FC: 2, SD FD: 3, SE FE: 4, SF FF: 5);
route_list!(7; SA FA: 0, SB FB: 1, SC FC: 2, SD FD: 3, SE FE: 4, SF FF: 5, SG FG: 6);
route_list!(8; SA FA: 0, SB FB: 1, SC FC: 2, SD FD: 3, SE FE: 4, SF FF: 5, SG FG: 6, SH FH: 7);
route_list!(9; SA FA: 0, SB FB: 1, SC FC: 2, SD FD: 3, SE FE: 4, SF FF: 5, SG FG: 6, SH FH: 7, SI FI: 8);
route_list!(10; SA FA: 0, SB FB: 1, SC FC: 2, SD FD: 3, SE FE: 4, SF FF: 5, SG FG: 6, SH FH: 7, SI FI: 8, SJ FJ: 9);
route_list!(11; SA FA: 0, SB FB: 1, SC FC: 2, SD FD: 3, SE FE: 4, SF FF: 5, SG FG: 6, SH FH: 7, SI FI: 8, SJ FJ: 9, SK FK: 10);
route_list!(12; SA FA: 0, SB FB: 1, SC FC: 2, SD FD: 3, SE FE: 4, SF FF: 5, SG FG: 6, SH FH: 7, SI FI: 8, SJ FJ: 9, SK FK: 10, SL FL: 11);
