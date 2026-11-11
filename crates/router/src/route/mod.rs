use fast_http_core::{
    DispatchOptions, Endpoint, EndpointSet, ErrorHandler, Flow, HandlerError, Identity, Next,
    PathParams, Rejection, ReqBody, Request, Response, Stack, TuplePush,
};

use crate::{FnEndpoint, Handler, ParamBuf, Pattern, PatternError, handler};

pub use self::into::{IntoRoute, attributed, same_path};
pub use self::list::{Append, Level, RouteList};

mod into;
mod list;

/// A route pattern together with the endpoints that answer it and the flow wrapped around them.
#[derive(Debug)]
pub struct Route<S, F = Identity> {
    pattern: Pattern,
    set: S,
    flow: F,
}

impl Route<()> {
    /// Parses `pattern`; add endpoints with [`Route::endpoint`] or the method shorthands.
    pub fn new(pattern: &'static str) -> Result<Self, PatternError> {
        Ok(Self {
            pattern: Pattern::parse(pattern)?,
            set: (),
            flow: Identity,
        })
    }
}

macro_rules! method_shorthand {
    ($($name:ident),+) => {
        $(
            #[doc = concat!("Appends a `", stringify!($name), "` endpoint calling `handler`.")]
            pub fn $name<H: Handler<M>, M: 'static>(self, handler: H) -> Route<S::Output, F>
            where
                S: TuplePush<FnEndpoint<H, M>>,
            {
                self.endpoint(handler::$name(handler))
            }
        )+
    };
}

impl<S, F> Route<S, F> {
    /// Appends `endpoint`; endpoints of a route are tried in the order they are added.
    pub fn endpoint<E: Endpoint>(self, endpoint: E) -> Route<S::Output, F>
    where
        S: TuplePush<E>,
    {
        Route {
            pattern: self.pattern,
            set: self.set.push(endpoint),
            flow: self.flow,
        }
    }

    method_shorthand!(get, post, put, patch, delete);

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
