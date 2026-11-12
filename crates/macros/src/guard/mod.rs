use syn::{Error, Expr, Ident, Pat};

use self::free::free_names;

use crate::expand::Param;

mod free;

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

/// Finds which parameters `expr` reads: the names their patterns bind that the expression uses
/// without binding them itself, as [`free_names`] finds them.
///
/// `headers` and `query` are reserved: when the expression names one and no parameter binds it,
/// the guard gets the view of the request head. A parameter of that name takes precedence.
///
/// `self` is rejected: a guard runs before the controller is consulted and can only see what the
/// method takes as parameters.
pub fn analyze<'a>(expr: &'a Expr, params: &[Param]) -> syn::Result<Analyzed<'a>> {
    let free = free_names(expr);

    if free.iter().any(|name| name == "self") {
        return Err(Error::new_spanned(
            expr,
            "a guard cannot use `self`; take what it needs as a parameter of the method",
        ));
    }

    let uses = params
        .iter()
        .enumerate()
        .filter(|(_, param)| param.bindings.iter().any(|binding| free.contains(binding)))
        .map(|(index, _)| index)
        .collect();

    let reserved = Reserved::ALL
        .into_iter()
        .filter(|(_, name)| {
            free.iter().any(|found| found == name)
                && !params
                    .iter()
                    .any(|param| param.bindings.iter().any(|binding| binding == name))
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

#[cfg(test)]
mod tests;
