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
///   the editor highlights and checks it. A guard cannot use `self`.
///
/// Guarded variants compile to one generated endpoint that does as little work as possible:
/// arguments are extracted in the order the guards need them, a type that several variants take is
/// extracted once and moved into the winning variant, and the request body is buffered only when
/// different variants need different body types.
///
/// The generated `routes(self: Arc<Self>)` returns the router, or the `PatternError` of the first
/// malformed route pattern.
#[proc_macro_attribute]
pub fn controller(args: TokenStream, item: TokenStream) -> TokenStream {
    expand::controller(args.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
