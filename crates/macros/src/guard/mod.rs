use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use syn::{Error, Expr, Ident, Pat};

use crate::expand::Param;

/// A guard together with the method parameters it mentions.
pub struct Analyzed<'a> {
    /// The expression, emitted exactly as written.
    pub expr: &'a Expr,
    /// Indexes of the parameters whose bindings the expression uses, ascending.
    pub uses: Vec<usize>,
}

/// Finds which parameters `expr` reads, by looking for the names their patterns bind.
///
/// `self` is rejected: a guard runs before the controller is consulted and can only see what the
/// method takes as parameters.
pub fn analyze<'a>(expr: &'a Expr, params: &[Param]) -> syn::Result<Analyzed<'a>> {
    let tokens = expr.to_token_stream();

    if mentions(
        tokens.clone(),
        &[Ident::new("self", proc_macro2::Span::call_site())],
    ) {
        return Err(Error::new_spanned(
            expr,
            "a guard cannot use `self`; take what it needs as a parameter of the method",
        ));
    }

    let uses = params
        .iter()
        .enumerate()
        .filter(|(_, param)| mentions(tokens.clone(), &param.bindings))
        .map(|(index, _)| index)
        .collect();

    Ok(Analyzed { expr, uses })
}

/// The identifiers a pattern binds.
pub fn bindings(pat: &Pat) -> Vec<Ident> {
    let mut found = Vec::new();

    collect(pat, &mut found);

    found
}

fn collect(pat: &Pat, found: &mut Vec<Ident>) {
    match pat {
        Pat::Ident(ident) => {
            found.push(ident.ident.clone());

            if let Some((_, inner)) = &ident.subpat {
                collect(inner, found);
            }
        }
        Pat::Reference(inner) => collect(&inner.pat, found),
        Pat::Paren(inner) => collect(&inner.pat, found),
        Pat::Type(inner) => collect(&inner.pat, found),
        Pat::Struct(inner) => inner
            .fields
            .iter()
            .for_each(|field| collect(&field.pat, found)),
        Pat::Tuple(inner) => inner.elems.iter().for_each(|elem| collect(elem, found)),
        Pat::TupleStruct(inner) => inner.elems.iter().for_each(|elem| collect(elem, found)),
        Pat::Slice(inner) => inner.elems.iter().for_each(|elem| collect(elem, found)),
        Pat::Or(inner) => inner
            .cases
            .first()
            .iter()
            .for_each(|case| collect(case, found)),
        _ => {}
    }
}

fn mentions(tokens: TokenStream, names: &[Ident]) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => names.contains(&ident),
        TokenTree::Group(group) => mentions(group.stream(), names),
        _ => false,
    })
}

#[cfg(test)]
mod tests;
