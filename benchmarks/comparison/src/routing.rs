//! The same route table expressed for fast-http, matchit and actix-router, plus the paths to look up.

use fast_http_router::{ParamBuf, Route, RouteList, Router};

/// One route in each router's syntax.
#[derive(Debug, Clone, Copy)]
pub struct RouteDef {
    /// fast-http pattern.
    pub fast_http: &'static str,
    /// matchit pattern.
    pub matchit: &'static str,
    /// actix-router pattern.
    pub actix: &'static str,
}

/// A path to resolve and the route position it must land on.
#[derive(Debug, Clone, Copy)]
pub struct Lookup {
    /// Stable identifier used in bench names.
    pub name: &'static str,
    /// Request path.
    pub path: &'static str,
    /// Expected position in [`ROUTES`].
    pub route: usize,
}

/// Twelve routes mixing literals, parameters and a catch-all, typical of a small REST API.
pub const ROUTES: [RouteDef; 12] = [
    route("/health", "/health", "/health"),
    route("/users", "/users", "/users"),
    route("/users/{id}", "/users/{id}", "/users/{id}"),
    route(
        "/users/{id}/posts",
        "/users/{id}/posts",
        "/users/{id}/posts",
    ),
    route(
        "/users/{id}/posts/{post}",
        "/users/{id}/posts/{post}",
        "/users/{id}/posts/{post}",
    ),
    route("/orgs/{org}", "/orgs/{org}", "/orgs/{org}"),
    route(
        "/orgs/{org}/repos/{repo}",
        "/orgs/{org}/repos/{repo}",
        "/orgs/{org}/repos/{repo}",
    ),
    route(
        "/orgs/{org}/repos/{repo}/issues/{issue}",
        "/orgs/{org}/repos/{repo}/issues/{issue}",
        "/orgs/{org}/repos/{repo}/issues/{issue}",
    ),
    route("/orders", "/orders", "/orders"),
    route("/orders/{id}", "/orders/{id}", "/orders/{id}"),
    route("/api/v1/status", "/api/v1/status", "/api/v1/status"),
    route("/static/{**path}", "/static/{*path}", "/static/{path}*"),
];

/// Paths covering a static hit, one to three parameters, the catch-all and a miss.
pub const LOOKUPS: [Lookup; 6] = [
    lookup("static", "/api/v1/status", 10),
    lookup("one_param", "/users/42", 2),
    lookup("two_params", "/users/42/posts/7", 4),
    lookup("three_params", "/orgs/rust-lang/repos/cargo/issues/1234", 7),
    lookup("catch_all", "/static/css/site/main.css", 11),
    lookup("miss", "/nothing/here", usize::MAX),
];

const fn route(fast_http: &'static str, matchit: &'static str, actix: &'static str) -> RouteDef {
    RouteDef {
        fast_http,
        matchit,
        actix,
    }
}

const fn lookup(name: &'static str, path: &'static str, route: usize) -> Lookup {
    Lookup { name, path, route }
}

async fn noop() -> &'static str {
    ""
}

/// fast-http router over [`ROUTES`] with no-op handlers.
pub fn fast_http() -> Router<impl RouteList> {
    let at = |index: usize| {
        Route::new(ROUTES[index].fast_http)
            .expect("valid route")
            .get(noop)
    };

    Router::new()
        .route(at(0))
        .route(at(1))
        .route(at(2))
        .route(at(3))
        .route(at(4))
        .route(at(5))
        .route(at(6))
        .route(at(7))
        .route(at(8))
        .route(at(9))
        .route(at(10))
        .route(at(11))
}

/// fast-http lookup: route position plus captured parameter count.
pub fn fast_http_lookup<L: RouteList>(router: &Router<L>, path: &str) -> Option<(usize, usize)> {
    let mut buf = ParamBuf::new();
    let index = router.lookup(path, &mut buf)?;

    Some((index, buf.as_slice().len()))
}

/// matchit router over [`ROUTES`].
pub fn matchit() -> matchit::Router<usize> {
    let mut router = matchit::Router::new();

    for (index, route) in ROUTES.iter().enumerate() {
        router.insert(route.matchit, index).expect("valid route");
    }

    router
}

/// matchit lookup: route position plus captured parameter count.
pub fn matchit_lookup(router: &matchit::Router<usize>, path: &str) -> Option<(usize, usize)> {
    let matched = router.at(path).ok()?;

    Some((*matched.value, matched.params.len()))
}

/// actix-router over [`ROUTES`].
pub fn actix() -> actix_router::Router<usize> {
    let mut builder = actix_router::Router::<usize>::build();

    for (index, route) in ROUTES.iter().enumerate() {
        builder.path(route.actix, index);
    }

    builder.finish()
}

/// actix-router lookup: route position plus captured parameter count.
pub fn actix_lookup(router: &actix_router::Router<usize>, path: &str) -> Option<(usize, usize)> {
    let mut resource = actix_router::Path::new(path);
    let (index, _) = router.recognize(&mut resource)?;

    Some((*index, resource.segment_count()))
}
