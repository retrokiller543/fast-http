use fast_http_core::{
    DefaultErrorHandler, DispatchOptions, ErrorHandler, Flow, HandlerError, Identity, Next,
    PathParams, Rejection, ReqBody, Request, Response, Routes, Stack, flow,
};

use crate::{ParamBuf, Pattern, RouteList};

/// Candidate routes for each literal first segment, computed once so a request never scans every route.
#[derive(Debug)]
struct Index {
    literal: Vec<(&'static str, Box<[u16]>)>,
    dynamic: Box<[u16]>,
}

impl Index {
    fn build<L: RouteList>(routes: &L) -> Self {
        let firsts: Vec<Option<&'static str>> = (0..L::LEN)
            .map(|index| routes.pattern(index).first_literal())
            .collect();
        let mut keys: Vec<&'static str> = firsts.iter().flatten().copied().collect();

        keys.sort_unstable();
        keys.dedup();

        let candidates = |key: Option<&str>| -> Box<[u16]> {
            (0..L::LEN)
                .filter(|&index| firsts[index].is_none() || firsts[index] == key)
                .map(|index| index as u16)
                .collect()
        };
        let literal = keys
            .into_iter()
            .map(|key| (key, candidates(Some(key))))
            .collect();

        Self {
            literal,
            dynamic: candidates(None),
        }
    }

    fn candidates(&self, first: &str) -> &[u16] {
        match self.literal.binary_search_by_key(&first, |entry| entry.0) {
            Ok(position) => &self.literal[position].1,
            Err(_) => &self.dynamic,
        }
    }
}

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

impl<L: RouteList> Router<L> {
    /// Builds the lookup index for `routes`, a tuple of up to 12 [`Route`](crate::Route)s tried in order.
    pub fn new(routes: L) -> Self {
        let index = Index::build(&routes);

        Self {
            routes,
            handler: DefaultErrorHandler,
            options: DispatchOptions::default(),
            index,
            flow: Identity,
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
            .map(|&candidate| usize::from(candidate))
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
