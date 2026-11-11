use syn::{Expr, Ident, LitStr, Pat, Type};

use crate::route::HttpMethod;

/// One extractor argument of a handler method.
pub struct Param {
    /// The pattern as written, which also names what guards can use.
    pub pat: Pat,
    /// The extractor type as written.
    pub ty: Type,
    /// The identifiers the pattern binds.
    pub bindings: Vec<Ident>,
}

/// A function that carries route attributes: a controller method or a free function.
pub struct Handler {
    /// The name it is called by.
    pub name: Ident,
    /// Whether it takes `&self`.
    pub receiver: bool,
    /// Its extractor arguments, in order.
    pub params: Vec<Param>,
}

/// One route attribute: a handler answering a method and pattern, perhaps behind guards.
pub struct Variant {
    /// Index into the controller's handlers.
    pub handler: usize,
    /// The HTTP method.
    pub method: HttpMethod,
    /// The full route pattern, prefix included.
    pub pattern: LitStr,
    /// Boolean expressions over the handler's parameters, all of which must hold.
    pub guards: Vec<Expr>,
    /// The `when` clause as written, for spotting duplicates.
    pub when_text: String,
}

/// The impl block a controller's methods belong to.
pub struct Owner<'a> {
    /// The controller type.
    pub self_ty: &'a Type,
    /// The impl block's generics.
    pub generics: &'a syn::Generics,
}

/// What the generators need to know about where the handlers live.
pub struct Context<'a> {
    /// Crate that provides the `router` and `core` modules.
    pub krate: &'a syn::Path,
    /// The impl block, or `None` for free functions, which have no receiver and no state.
    pub owner: Option<Owner<'a>>,
    /// The base path routes are joined under at startup, such as a controller group's
    /// `<Self as Controller>::PATH`; `None` when the patterns are complete.
    pub base: Option<proc_macro2::TokenStream>,
}
