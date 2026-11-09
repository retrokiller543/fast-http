use fast_http_core::{
    DefaultErrorHandler, DispatchOptions, ErrorHandler, HandlerError, PathParams, Rejection,
    ReqBody, Request, Response, Routes,
};

use crate::{ParamBuf, RouteList};

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
#[derive(Debug)]
pub struct Router<L, H = DefaultErrorHandler> {
    routes: L,
    handler: H,
    options: DispatchOptions,
    index: Index,
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
        }
    }
}

impl<L, H> Router<L, H> {
    /// Replaces the central error handler.
    pub fn with_error_handler<N: ErrorHandler>(self, handler: N) -> Router<L, N> {
        Router {
            routes: self.routes,
            handler,
            options: self.options,
            index: self.index,
        }
    }

    /// Replaces the dispatch limits.
    pub fn with_options(mut self, options: DispatchOptions) -> Self {
        self.options = options;

        self
    }
}

impl<L: RouteList, H: ErrorHandler> Routes for Router<L, H> {
    async fn dispatch<B: ReqBody>(&self, req: Request<B>) -> Response {
        let (parts, body) = req.into_parts();
        let path = parts.uri.path();
        let first = path
            .strip_prefix('/')
            .unwrap_or(path)
            .split('/')
            .next()
            .unwrap_or_default();
        let mut buf = ParamBuf::new();

        for &candidate in self.index.candidates(first) {
            let index = usize::from(candidate);
            let pattern = self.routes.pattern(index);

            if !pattern.matches(path, &mut buf) {
                continue;
            }

            let params = PathParams::new(pattern.names(), buf.as_slice());

            return self
                .routes
                .call(index, &parts, params, body, &self.handler, self.options)
                .await;
        }

        self.handler
            .handle(HandlerError::new(Rejection::NotFound), &parts)
            .await
    }
}

#[cfg(test)]
mod tests;
