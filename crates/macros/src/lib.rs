//! Controller macros for fast-http.
//!
//! [`controller`] turns an `impl` block of `async fn` handlers into a `routes` constructor that
//! builds a `fast_http_router::Router` from the methods' route attributes. The expansion is plain
//! builder calls, so everything it generates can also be written by hand.

mod args;
mod expand;
mod path;
mod route;
mod when;

use proc_macro::TokenStream;

/// Builds routes from the `async fn`s of an inherent `impl` block.
///
/// ```ignore
/// #[controller("/users")]
/// impl UserApi {
///     #[get("/{id}")]
///     async fn show(&self, Path(id): Path<u64>) -> String { /* ... */ }
///
///     #[post("/chat", when(query.stream = true))]
///     async fn chat_stream(&self, body: Body<ChatReq>) -> Sse<ChatChunks> { /* ... */ }
/// }
///
/// let router = Arc::new(api).routes()?;
/// ```
///
/// - The optional argument is a path prefix. `crate = path` names the crate that provides the
///   facade modules `router` and `core`; it defaults to `::fast_http`.
/// - A method marked `#[get]`, `#[post]`, `#[put]`, `#[patch]` or `#[delete]` becomes an
///   endpoint at the given path, or at the prefix itself when the path is omitted; several attributes on one method add several endpoints. Unmarked methods are left
///   alone.
/// - A `&self` method is bound to the `Arc<Self>` that `routes` takes. A method without a
///   receiver is used as it is. Every other argument is an extractor.
/// - Endpoints that share a path form one route, tried in declaration order. `when(..)` restricts
///   an endpoint to requests whose head matches: `query.name`, `query.name = value`, `header.name`,
///   `header.name = "value"`, `not(..)`, `any(..)`, `all(..)` and `guard(Type)` for a
///   `HeadGuard`. Several conditions in one `when` must all hold.
///
/// The generated `routes(self: Arc<Self>)` returns the router, or the `PatternError` of the first
/// malformed route pattern.
#[proc_macro_attribute]
pub fn controller(args: TokenStream, item: TokenStream) -> TokenStream {
    expand::controller(args.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
