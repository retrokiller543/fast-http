use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{Attribute, Meta, Token, punctuated::Punctuated};

/// The conditional compilation a handler is under: the predicates of its `#[cfg(..)]`
/// attributes, and of the `cfg(..)` its `#[cfg_attr(..)]` attributes may apply, all of which
/// must hold for it to exist.
#[derive(Clone, Default)]
pub struct Gate(Vec<TokenStream>);

impl Gate {
    /// The gate `attrs` put a function under.
    ///
    /// `#[cfg(p)]` contributes `p`. `#[cfg_attr(c, cfg(p))]` contributes `any(not(c), p)`, since
    /// it removes the function only when `c` holds and `p` does not; nested `cfg_attr` resolve
    /// the same way. Attributes that apply no `cfg` contribute nothing.
    pub fn of(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut predicates = Vec::new();

        for attr in attrs {
            if let Some(predicate) = applied(&attr.meta)? {
                predicates.push(predicate);
            }
        }

        Ok(Self(predicates))
    }

    /// Whether nothing gates the function.
    pub fn is_open(&self) -> bool {
        self.0.is_empty()
    }

    /// The single predicate that holds when the function exists.
    pub fn predicate(&self) -> TokenStream {
        let predicates = &self.0;

        quote!(all(#(#predicates),*))
    }

    /// The `#[cfg(..)]` that gates an item generated for the function, or nothing when it is
    /// open.
    pub fn attr(&self) -> Option<TokenStream> {
        let predicate = self.predicate();

        (!self.is_open()).then(|| quote!(#[cfg(#predicate)]))
    }

    /// The gate as text, to tell whether two functions are under the same one.
    pub fn key(&self) -> String {
        self.predicate().to_string()
    }
}

/// The `#[cfg(..)]` of something that exists when any of `gates` is satisfied, or nothing when
/// one of them is open.
pub fn any_of(gates: &[Gate]) -> Option<TokenStream> {
    if gates.is_empty() || gates.iter().any(Gate::is_open) {
        return None;
    }

    let predicates = gates.iter().map(Gate::predicate);

    Some(quote!(#[cfg(any(#(#predicates),*))]))
}

/// The predicate under which `meta` keeps the item, when it is a `cfg` or a `cfg_attr` that
/// applies one.
fn applied(meta: &Meta) -> syn::Result<Option<TokenStream>> {
    let Meta::List(list) = meta else {
        return Ok(None);
    };

    if list.path.is_ident("cfg") {
        return Ok(Some(list.tokens.clone()));
    }

    if !list.path.is_ident("cfg_attr") {
        return Ok(None);
    }

    let args = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
    let mut args = args.iter();
    let Some(condition) = args.next() else {
        return Ok(None);
    };
    let mut inner = Vec::new();

    for arg in args {
        if let Some(predicate) = applied(arg)? {
            inner.push(predicate);
        }
    }

    if inner.is_empty() {
        return Ok(None);
    }

    let condition = condition.to_token_stream();

    Ok(Some(quote!(any(not(#condition), all(#(#inner),*)))))
}

#[cfg(test)]
mod tests;
