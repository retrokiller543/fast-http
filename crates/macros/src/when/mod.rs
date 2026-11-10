use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Error, Ident, Lit, LitStr, Path, Token,
    ext::IdentExt,
    parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

/// One condition of a `when(..)` clause, evaluated on the request head.
pub enum Condition {
    /// The query has the parameter, optionally with exactly this value.
    Query(String, Option<String>),
    /// The request has the header, optionally with exactly this value.
    Header(String, Option<String>),
    /// The inner condition does not hold.
    Not(Box<Condition>),
    /// At least one inner condition holds.
    Any(Vec<Condition>),
    /// Every inner condition holds.
    All(Vec<Condition>),
    /// The [`HeadGuard`](fast_http_core::HeadGuard) type matches.
    Guard(Path),
}

impl Condition {
    /// The `bool` expression over a `Parts` named `parts` that evaluates the condition.
    pub fn expression(&self, krate: &Path) -> TokenStream {
        let guard = quote!(#krate::core::guard);

        match self {
            Self::Query(name, None) => quote!(#guard::query_has(parts, #name)),
            Self::Query(name, Some(value)) => quote!(#guard::query_eq(parts, #name, #value)),
            Self::Header(name, None) => quote!(#guard::header_has(parts, #name)),
            Self::Header(name, Some(value)) => quote!(#guard::header_eq(parts, #name, #value)),
            Self::Not(inner) => {
                let inner = inner.expression(krate);

                quote!(!(#inner))
            }
            Self::Any(inner) => join(inner, krate, quote!(||)),
            Self::All(inner) => join(inner, krate, quote!(&&)),
            Self::Guard(path) => {
                quote!(<#path as #krate::core::HeadGuard>::check(parts).is_some())
            }
        }
    }
}

fn join(inner: &[Condition], krate: &Path, operator: TokenStream) -> TokenStream {
    let mut expressions = inner.iter().map(|condition| condition.expression(krate));
    let first = expressions.next().unwrap_or_default();
    let joined = expressions.fold(first, |left, right| quote!(#left #operator #right));

    quote!((#joined))
}

fn name(input: ParseStream, header: bool) -> syn::Result<String> {
    let name = if input.peek(LitStr) {
        input.parse::<LitStr>()?.value()
    } else {
        input.call(Ident::parse_any)?.to_string()
    };

    if header {
        return Ok(name.replace('_', "-").to_ascii_lowercase());
    }

    Ok(name)
}

fn value(input: ParseStream) -> syn::Result<Option<String>> {
    if !input.peek(Token![=]) {
        return Ok(None);
    }

    input.parse::<Token![=]>()?;

    match input.parse::<Lit>()? {
        Lit::Str(text) => Ok(Some(text.value())),
        Lit::Int(number) => Ok(Some(number.base10_digits().to_owned())),
        Lit::Bool(flag) => Ok(Some(flag.value.to_string())),
        other => Err(Error::new_spanned(
            other,
            "expected a string, integer or boolean literal",
        )),
    }
}

fn nested(input: ParseStream, keyword: &Ident) -> syn::Result<Vec<Condition>> {
    let content;

    parenthesized!(content in input);

    let list = Punctuated::<Condition, Token![,]>::parse_terminated(&content)?;

    if list.is_empty() {
        return Err(Error::new_spanned(
            keyword,
            format!("`{keyword}(..)` needs at least one condition"),
        ));
    }

    Ok(list.into_iter().collect())
}

impl Parse for Condition {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let keyword: Ident = input.call(Ident::parse_any)?;

        match keyword.to_string().as_str() {
            "query" => {
                input.parse::<Token![.]>()?;

                Ok(Self::Query(name(input, false)?, value(input)?))
            }
            "header" => {
                input.parse::<Token![.]>()?;

                Ok(Self::Header(name(input, true)?, value(input)?))
            }
            "not" => {
                let mut inner = nested(input, &keyword)?;

                if inner.len() != 1 {
                    return Err(Error::new_spanned(
                        &keyword,
                        "`not(..)` takes exactly one condition",
                    ));
                }

                Ok(Self::Not(Box::new(inner.remove(0))))
            }
            "any" => Ok(Self::Any(nested(input, &keyword)?)),
            "all" => Ok(Self::All(nested(input, &keyword)?)),
            "guard" => {
                let content;

                parenthesized!(content in input);

                Ok(Self::Guard(content.parse()?))
            }
            "body" => Err(Error::new_spanned(
                &keyword,
                "body guards are not supported by `when(..)`; implement `Endpoint` by hand",
            )),
            _ => Err(Error::new_spanned(
                &keyword,
                "expected `query`, `header`, `not`, `any`, `all` or `guard`",
            )),
        }
    }
}

#[cfg(test)]
mod tests;
