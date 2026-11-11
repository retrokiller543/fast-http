use fast_http_core::{EndpointSet, Flow};

use super::Route;

/// Anything [`Router::route`](crate::Router::route) accepts: a [`Route`], or a function that a
/// route attribute such as `#[get("/path")]` turned into an endpoint, which becomes the route at
/// its own path.
pub trait IntoRoute {
    /// The endpoints of the route.
    type Set: EndpointSet;
    /// The flow wrapped around them.
    type Flow: Flow;

    /// Builds the route.
    fn into_route(self) -> Route<Self::Set, Self::Flow>;
}

impl<S: EndpointSet, F: Flow> IntoRoute for Route<S, F> {
    type Set = S;
    type Flow = F;

    fn into_route(self) -> Self {
        self
    }
}

/// The route at `pattern`, which a route attribute already checked at compile time.
///
/// # Panics
///
/// When `pattern` is not a valid pattern, which the attribute rules out.
#[doc(hidden)]
pub fn attributed(pattern: &'static str) -> Route<()> {
    match Route::new(pattern) {
        Ok(route) => route,
        Err(error) => panic!("a route attribute let an invalid pattern through: {error}"),
    }
}

/// Whether two route attributes name the same path; `const` so [`endpoints!`](crate::endpoints)
/// can refuse a mismatch at compile time.
#[doc(hidden)]
pub const fn same_path(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    let mut index = 0;

    if left.len() != right.len() {
        return false;
    }

    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }

        index += 1;
    }

    true
}

/// One route whose endpoints are the given attributed functions, tried in order.
///
/// Every function must carry a route attribute for the same path, which becomes the route's path;
/// a function routed elsewhere is a compile error. A guarded function that misses declines, so the
/// next one gets the request.
///
/// ```ignore
/// #[get("/greet", when(query.contains("name")))]
/// async fn greet_named(query: QueryParams<'_>) -> String { /* ... */ }
///
/// #[get("/greet")]
/// async fn greet() -> &'static str { "hello" }
///
/// Router::new().route(endpoints![greet_named, greet])
/// ```
#[macro_export]
macro_rules! endpoints {
    ($first:path $(, $rest:path)* $(,)?) => {{
        $(
            const _: () = ::core::assert!(
                $crate::route::same_path(<$first>::PATH, <$rest>::PATH),
                ::core::concat!(
                    "`", ::core::stringify!($rest), "` is routed at a different path than `",
                    ::core::stringify!($first), "`"
                ),
            );
        )*

        $crate::route::attributed(<$first>::PATH)
            .endpoint($first)
            $(.endpoint($rest))*
    }};
}

#[cfg(test)]
mod tests;
