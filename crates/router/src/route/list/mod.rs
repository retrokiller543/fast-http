use fast_http_core::{
    DispatchOptions, EndpointSet, ErrorHandler, Flow, PathParams, ReqBody, Request, Response, flow,
};
use http::request::Parts;

use super::{Route, RouteNext};
use crate::Pattern;

pub use self::level::{Append, Level};

mod level;

/// The routes of a router, statically typed and addressed by position.
///
/// Implemented for a single [`Route`], `()`, tuples of up to 16 route lists and [`Level`]s.
/// Positions run through the elements of a tuple in order.
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

impl<S: EndpointSet, F: Flow> RouteList for Route<S, F> {
    const LEN: usize = 1;

    fn pattern(&self, _: usize) -> &Pattern {
        &self.pattern
    }

    fn passthrough(&self, _: usize) -> bool {
        F::PASSTHROUGH
    }

    async fn call<B: ReqBody, H: ErrorHandler>(
        &self,
        _: usize,
        parts: &Parts,
        params: PathParams<'_>,
        body: B,
        handler: &H,
        options: DispatchOptions,
    ) -> Response {
        fast_http_core::dispatch_set(&self.set, parts, params, body, handler, options).await
    }

    async fn call_layered<B: ReqBody, H: ErrorHandler>(
        &self,
        _: usize,
        req: Request<B>,
        handler: &H,
        options: DispatchOptions,
    ) -> Response {
        let next = RouteNext {
            pattern: &self.pattern,
            set: &self.set,
            handler,
            options,
        };

        flow::run(&self.flow, req, next, handler).await
    }
}

impl RouteList for () {
    const LEN: usize = 0;

    fn pattern(&self, _: usize) -> &Pattern {
        unreachable!("an empty route list has no positions")
    }

    fn passthrough(&self, _: usize) -> bool {
        unreachable!("an empty route list has no positions")
    }

    async fn call<B: ReqBody, H: ErrorHandler>(
        &self,
        _: usize,
        _: &Parts,
        _: PathParams<'_>,
        _: B,
        _: &H,
        _: DispatchOptions,
    ) -> Response {
        unreachable!("an empty route list has no positions")
    }

    async fn call_layered<B: ReqBody, H: ErrorHandler>(
        &self,
        _: usize,
        _: Request<B>,
        _: &H,
        _: DispatchOptions,
    ) -> Response {
        unreachable!("an empty route list has no positions")
    }
}

macro_rules! route_list {
    ($($list:ident : $index:tt),+) => {
        impl<$($list: RouteList),+> RouteList for ($($list,)+) {
            const LEN: usize = 0 $(+ $list::LEN)+;

            fn pattern(&self, index: usize) -> &Pattern {
                let mut index = index;

                $(
                    if index < $list::LEN {
                        return self.$index.pattern(index);
                    }

                    index -= $list::LEN;
                )+

                unreachable!("position {index} is past the end of the route list")
            }

            fn passthrough(&self, index: usize) -> bool {
                let mut index = index;

                $(
                    if index < $list::LEN {
                        return self.$index.passthrough(index);
                    }

                    index -= $list::LEN;
                )+

                unreachable!("position {index} is past the end of the route list")
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
                let mut index = index;

                $(
                    if index < $list::LEN {
                        return self.$index.call(index, parts, params, body, handler, options).await;
                    }

                    index -= $list::LEN;
                )+

                unreachable!("position {index} is past the end of the route list")
            }

            async fn call_layered<Bd: ReqBody, Eh: ErrorHandler>(
                &self,
                index: usize,
                req: Request<Bd>,
                handler: &Eh,
                options: DispatchOptions,
            ) -> Response {
                let mut index = index;

                $(
                    if index < $list::LEN {
                        return self.$index.call_layered(index, req, handler, options).await;
                    }

                    index -= $list::LEN;
                )+

                unreachable!("position {index} is past the end of the route list")
            }
        }
    };
}

route_list!(A: 0);
route_list!(A: 0, B: 1);
route_list!(A: 0, B: 1, C: 2);
route_list!(A: 0, B: 1, C: 2, D: 3);
route_list!(A: 0, B: 1, C: 2, D: 3, E: 4);
route_list!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5);
route_list!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6);
route_list!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7);
route_list!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8);
route_list!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9);
route_list!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10);
route_list!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10, L: 11);
route_list!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10, L: 11, M: 12);
route_list!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10, L: 11, M: 12, N: 13);
route_list!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10, L: 11, M: 12, N: 13, O: 14);
route_list!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10, L: 11, M: 12, N: 13, O: 14, P: 15);
