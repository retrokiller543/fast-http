use proc_macro2::{Spacing, TokenStream, TokenTree};
use quote::ToTokens;
use syn::{Error, Expr, Ident, Pat};

use crate::expand::Param;

/// A view of the request head that a guard may name without the method taking it as a parameter.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Reserved {
    /// `headers`: the request's `&HeaderMap`.
    Headers,
    /// `query`: a `QueryParams` view of the query string.
    Query,
}

impl Reserved {
    const ALL: [(Self, &'static str); 2] = [(Self::Headers, "headers"), (Self::Query, "query")];
}

/// A guard together with the method parameters it mentions.
pub struct Analyzed<'a> {
    /// The expression, emitted exactly as written.
    pub expr: &'a Expr,
    /// Indexes of the parameters whose bindings the expression uses, ascending.
    pub uses: Vec<usize>,
    /// The reserved names the expression uses that no parameter of the method binds, so the
    /// guard builds the view itself.
    pub reserved: Vec<Reserved>,
}

/// Finds which parameters `expr` reads, by looking for the names their patterns bind.
///
/// `headers` and `query` are reserved: when the expression names one and no parameter binds it,
/// the guard gets the view of the request head. A parameter of that name takes precedence.
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

    let reserved = Reserved::ALL
        .into_iter()
        .filter(|(_, name)| {
            let ident = Ident::new(name, proc_macro2::Span::call_site());

            mentions(tokens.clone(), std::slice::from_ref(&ident))
                && !params.iter().any(|param| param.bindings.contains(&ident))
        })
        .map(|(reserved, _)| reserved)
        .collect();

    Ok(Analyzed {
        expr,
        uses,
        reserved,
    })
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

/// Whether `tokens` use one of `names` as a binding.
///
/// The tokens are scanned rather than parsed, because guards are often macro calls such as
/// `matches!(..)` whose arguments are not expressions. A name right after `.` or `::` is a field,
/// method or path segment, as in `headers.contains_key(..)`, and a name right before a lone `:`
/// is a field, as in `Job::Fast { id: 0, .. }`; neither refers to a binding.
fn mentions(tokens: TokenStream, names: &[Ident]) -> bool {
    let tokens: Vec<_> = tokens.into_iter().collect();

    tokens.iter().enumerate().any(|(index, token)| match token {
        TokenTree::Ident(ident) => {
            names.contains(ident)
                && !is_member(&tokens[..index])
                && !is_field_name(tokens.get(index + 1))
        }
        TokenTree::Group(group) => mentions(group.stream(), names),
        _ => false,
    })
}

/// Whether the tokens before a name make it a member or path segment: they end in a `.` that is
/// not part of `..`, or in `::`.
fn is_member(before: &[TokenTree]) -> bool {
    let punct = |token: Option<&TokenTree>, ch: char| matches!(token, Some(TokenTree::Punct(punct)) if punct.as_char() == ch);
    let joint = |token: Option<&TokenTree>, ch: char| {
        matches!(
            token,
            Some(TokenTree::Punct(punct)) if punct.as_char() == ch && punct.spacing() == Spacing::Joint
        )
    };
    let last = before.last();
    let previous = before
        .len()
        .checked_sub(2)
        .and_then(|index| before.get(index));

    (punct(last, '.') && !joint(previous, '.')) || (punct(last, ':') && joint(previous, ':'))
}

/// Whether the token after a name makes it a field name: a `:` that does not start `::`.
fn is_field_name(next: Option<&TokenTree>) -> bool {
    matches!(
        next,
        Some(TokenTree::Punct(punct)) if punct.as_char() == ':' && punct.spacing() == Spacing::Alone
    )
}

#[cfg(test)]
mod tests;
