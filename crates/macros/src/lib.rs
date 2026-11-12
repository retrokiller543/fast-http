//! Controller macros for fast-http.
//!
//! [`controller`] turns an `impl` block of `async fn` handlers into a `routes` constructor that
//! builds a `fast_http_router::Router` from the methods' route attributes. The expansion is plain
//! builder calls, so everything it generates can also be written by hand.

mod args;
mod expand;
mod guard;
mod path;
mod route;

use proc_macro::TokenStream;

/// Builds routes from the `async fn`s of an inherent `impl` block.
///
/// ```ignore
/// #[controller("/chat")]
/// impl Chat {
///     #[post(when(params.stream == Some(true)))]
///     async fn stream(&self, Query(params): Query<Params>, body: Body<ChatReq>) -> Sse<ChatChunks> { /* ... */ }
///
///     #[post]
///     async fn buffered(&self, body: Body<ChatReq>) -> Negotiated<ChatRes> { /* ... */ }
/// }
///
/// let router = Router::new().merge(Arc::new(chat).routes()?);
/// ```
///
/// - The optional argument is a path prefix. `crate = path` names the crate that provides the
///   facade modules `router` and `core`; it defaults to `::fast_http`.
/// - A method marked `#[get]`, `#[post]`, `#[put]`, `#[patch]` or `#[delete]` becomes an
///   endpoint at the given path, or at the prefix itself when the path is omitted; several
///   attributes on one method add several endpoints. Unmarked methods are left alone.
/// - A `&self` method is bound to the `Arc<Self>` that `routes` takes. A method without a
///   receiver is used as it is. Every other argument is an extractor.
/// - Endpoints that share a method and path form one route, tried in declaration order.
///   `when(..)` holds boolean Rust expressions over the method's own parameters, all of which
///   must hold. Parameters are borrowed in a guard and the expression is emitted as written, so
///   the editor highlights and checks it. `headers` and `query` are reserved: a guard that uses
///   one without the method declaring a parameter of that name gets the request's `&HeaderMap`
///   or a `QueryParams` view. A guard cannot use `self`.
///
/// Guarded variants compile to one generated endpoint that does as little work as possible:
/// arguments are extracted in the order the guards need them, a type that several variants take is
/// extracted once and moved into the winning variant, and the request body is buffered only when
/// different variants need different body types.
///
/// The generated `routes(self: Arc<Self>)` returns the router, or the `PatternError` of the first
/// malformed route pattern.
///
/// A controller can also span several impl blocks. On a struct, `#[controller("/path",
/// groups(a, b))]` implements `fast_http_router::Controller` and adds a `routes` that merges the
/// listed groups; each `#[controller(group = a, "/prefix")]` impl block is one of them, under the
/// struct's path and its own optional prefix. A listed group without a block fails to compile, and
/// a block nobody lists leaves its handlers unused, which warns.
#[proc_macro_attribute]
pub fn controller(args: TokenStream, item: TokenStream) -> TokenStream {
    let original: proc_macro2::TokenStream = item.clone().into();

    match expand::controller(args.into(), item.into()) {
        Ok(expanded) => expanded.into(),
        Err(error) => expand::recover(original, error).into(),
    }
}

/// Builds routes from the free `async fn`s of an inline module.
///
/// ```ignore
/// #[routes("/items")]
/// mod items {
///     use super::*;
///
///     #[get("/{id}", when(*id == 0))]
///     async fn show_zero(Path(id): Path<u64>) -> &'static str { "zero" }
///
///     #[get("/{id}")]
///     async fn show(Path(id): Path<NonZeroU64>) -> String { /* ... */ }
/// }
///
/// let router = Router::new().merge(items::routes()?);
/// ```
///
/// This is [`controller`] without state: the same attributes, guards and lazy, extract-once
/// dispatch, over functions that have no `self`. The module gets a `routes()` function. Functions
/// that share a method and path are variants of one route, in declaration order.
#[proc_macro_attribute]
pub fn routes(args: TokenStream, item: TokenStream) -> TokenStream {
    let original: proc_macro2::TokenStream = item.clone().into();

    match expand::routes(args.into(), item.into()) {
        Ok(expanded) => expanded.into(),
        Err(error) => expand::recover(original, error).into(),
    }
}

macro_rules! standalone_routes {
    ($($name:ident => $method:ident),+ $(,)?) => {
        $(
            #[doc = concat!("Makes a free `async fn` the endpoint for a `", stringify!($name), "` route.")]
            ///
            /// The function's name becomes a unit struct that implements `Endpoint`, and
            /// `name.route()` builds the route at the attribute's path. `when(..)` takes the
            /// same guards as in [`controller`]. To give one path several guarded variants,
            /// put the functions in a [`routes`] module.
            ///
            /// ```ignore
            /// #[get("/health", when(query.contains("deep")))]
            /// async fn deep_health() -> &'static str { "deep" }
            ///
            /// let router = Router::new().route(deep_health.route()?);
            /// ```
            #[proc_macro_attribute]
            pub fn $name(args: TokenStream, item: TokenStream) -> TokenStream {
                let original: proc_macro2::TokenStream = item.clone().into();

                match expand::standalone(route::HttpMethod::$method, args.into(), item.into()) {
                    Ok(expanded) => expanded.into(),
                    Err(error) => {
                        let error = error.into_compile_error();

                        quote::quote!(#original #error).into()
                    }
                }
            }
        )+
    };
}

standalone_routes!(get => Get, post => Post, put => Put, patch => Patch, delete => Delete);

/// The general form of a route: `#[route(METHOD, "/path", when(..))]`.
///
/// `METHOD` is `GET`, `POST`, `PUT`, `PATCH` or `DELETE`, in any case. `#[get(..)]` and its
/// siblings are shorthands for it, and it works wherever they do: on a lone free function, on a
/// method of a [`controller`] and on a function of a [`routes`] module.
///
/// ```ignore
/// #[route(GET, "/health", when(query.contains("deep")))]
/// async fn deep_health() -> &'static str { "deep" }
/// ```
#[proc_macro_attribute]
pub fn route(args: TokenStream, item: TokenStream) -> TokenStream {
    let original: proc_macro2::TokenStream = item.clone().into();

    match expand::standalone_route(args.into(), item.into()) {
        Ok(expanded) => expanded.into(),
        Err(error) => {
            let error = error.into_compile_error();

            quote::quote!(#original #error).into()
        }
    }
}
