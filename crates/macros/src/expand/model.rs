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

/// A controller method that carries route attributes.
pub struct Handler {
    /// The method's name.
    pub name: Ident,
    /// Whether it takes `&self`.
    pub receiver: bool,
    /// Its extractor arguments, in order.
    pub params: Vec<Param>,
    /// Its declared return type.
    pub output: syn::ReturnType,
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

/// What the generators need to know about the impl block.
pub struct Context<'a> {
    /// Crate that provides the `router` and `core` modules.
    pub krate: &'a syn::Path,
    /// The controller type.
    pub self_ty: &'a Type,
    /// The impl block's generics.
    pub generics: &'a syn::Generics,
}
