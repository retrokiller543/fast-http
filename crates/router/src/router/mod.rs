use fast_http_core::{
    DefaultErrorHandler, DispatchOptions, ErrorHandler, Flow, HandlerError, Identity, Next,
    PathParams, Rejection, ReqBody, Request, Response, Routes, Stack, flow,
};

use crate::{Append, IntoRoute, ParamBuf, Pattern, Route, RouteList};

use self::index::Index;

mod index;

/// Statically typed router: tries the first matching route and sends misses through the error handler.
///
/// `F` is the flow wrapped around the whole router; it sees every request before matching.
#[derive(Debug)]
pub struct Router<L, H = DefaultErrorHandler, F = Identity> {
    routes: L,
    handler: H,
    options: DispatchOptions,
    index: Index,
    flow: F,
}

impl Router<()> {
    /// An empty router; add routes with [`Router::route`]. Every request is a 404 until then.
    pub fn new() -> Self {
        Self {
            routes: (),
            handler: DefaultErrorHandler,
            options: DispatchOptions::default(),
            index: Index::default(),
            flow: Identity,
        }
    }
}

impl Default for Router<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<L: RouteList, H, F> Router<L, H, F> {
    /// Appends `route`.
    ///
    /// Takes a [`Route`], or a function a route attribute turned into an endpoint, which becomes
    /// the route at its own path. A request goes to the most specific route that matches, so
    /// `/items/orders` wins over `/items/{id}` whatever order they are added in; equally specific
    /// routes are tried in the order they are added.
    ///
    /// # Panics
    ///
    /// When the router already holds `u32::MAX` routes, or when `route` matches exactly the paths
    /// of a route added before it, such as `/a/{id}` after `/a/{key}`: the router never falls
    /// through to a later route, so one of them could not be reached.
    pub fn route<R: IntoRoute>(self, route: R) -> Router<L::Output, H, F>
    where
        L: Append<Route<R::Set, R::Flow>>,
    {
        let mut index = self.index;
        let routes = self.routes.append(route.into_route());

        index.push(L::LEN, |position| routes.pattern(position));

        Router {
            routes,
            handler: self.handler,
            options: self.options,
            index,
            flow: self.flow,
        }
    }
}

impl<L: RouteList, H, F> Router<L, H, F> {
    /// Appends the routes of `group` after this router's own, keeping their order.
    ///
    /// Building a large route table as one long [`route`](Router::route) chain makes every step a
    /// bigger type, so compile time and debug-build stack use grow with the square of the chain
    /// length. Build routes in groups, such as one function per controller, and merge them.
    ///
    /// `group` joins as one element, contributing only its routes; give it no error handler,
    /// flows or options.
    ///
    /// # Panics
    ///
    /// When the merged router would hold more than `u32::MAX` routes, or when a route of `group`
    /// matches exactly the paths of a route already in this router.
    pub fn merge<M: RouteList>(self, group: Router<M>) -> Router<L::Output, H, F>
    where
        L: Append<M>,
    {
        let mut index = self.index;
        let routes = self.routes.append(group.routes);

        for position in L::LEN..L::LEN + M::LEN {
            index.push(position, |position| routes.pattern(position));
        }

        Router {
            routes,
            handler: self.handler,
            options: self.options,
            index,
            flow: self.flow,
        }
    }
}

impl<L, H, F> Router<L, H, F> {
    /// Replaces the central error handler, which also renders errors raised by flows.
    pub fn with_error_handler<N: ErrorHandler>(self, handler: N) -> Router<L, N, F> {
        Router {
            routes: self.routes,
            handler,
            options: self.options,
            index: self.index,
            flow: self.flow,
        }
    }

    /// Wraps `flow` around the whole router; it runs before matching, and the last one added runs first.
    pub fn layer<G: Flow>(self, flow: G) -> Router<L, H, Stack<F, G>> {
        Router {
            routes: self.routes,
            handler: self.handler,
            options: self.options,
            index: self.index,
            flow: Stack::new(self.flow, flow),
        }
    }

    /// Replaces the dispatch limits.
    pub fn with_options(mut self, options: DispatchOptions) -> Self {
        self.options = options;

        self
    }
}

impl<L: RouteList, H, F> Router<L, H, F> {
    /// Finds the first route matching `path`, filling `buf` with its captured values.
    ///
    /// Returns the route's position; [`Router::pattern`] gives the parameter names for `buf`.
    pub fn lookup<'p>(&self, path: &'p str, buf: &mut ParamBuf<'p>) -> Option<usize> {
        let first = path
            .strip_prefix('/')
            .unwrap_or(path)
            .split('/')
            .next()
            .unwrap_or_default();

        self.index
            .candidates(first)
            .iter()
            .map(|&candidate| candidate as usize)
            .find(|&index| self.routes.pattern(index).matches(path, buf))
    }

    /// The pattern of the route at `index`.
    pub fn pattern(&self, index: usize) -> &Pattern {
        self.routes.pattern(index)
    }
}

impl<L: RouteList, H: ErrorHandler, F: Flow> Routes for Router<L, H, F> {
    fn dispatch<B: ReqBody>(&self, req: Request<B>) -> impl Future<Output = Response> + Send {
        flow::run(&self.flow, req, Matcher { router: self }, &self.handler)
    }
}

/// The inside of the router-level flow: matching and the matched route.
struct Matcher<'a, L, H, F> {
    router: &'a Router<L, H, F>,
}

impl<L: RouteList, H: ErrorHandler, F: Flow> Next for Matcher<'_, L, H, F> {
    async fn run<B: ReqBody>(self, req: Request<B>) -> Response {
        let router = self.router;
        let (parts, body) = req.into_parts();
        let mut buf = ParamBuf::new();

        let Some(index) = router.lookup(parts.uri.path(), &mut buf) else {
            return router
                .handler
                .handle(HandlerError::new(Rejection::NotFound), &parts)
                .await;
        };

        if !router.routes.passthrough(index) {
            let req = Request::from_parts(parts, body);

            return router
                .routes
                .call_layered(index, req, &router.handler, router.options)
                .await;
        }

        let params = PathParams::new(router.routes.pattern(index).names(), buf.as_slice());

        router
            .routes
            .call(index, &parts, params, body, &router.handler, router.options)
            .await
    }
}

#[cfg(test)]
mod tests;
