use proc_macro2::{Span, TokenStream};
use syn::{
    Attribute, Expr, Ident, LitStr, Meta, Token, parenthesized, parse::ParseStream,
    punctuated::Punctuated,
};

/// The HTTP methods a route attribute can name.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    /// `#[get]`.
    Get,
    /// `#[post]`.
    Post,
    /// `#[put]`.
    Put,
    /// `#[patch]`.
    Patch,
    /// `#[delete]`.
    Delete,
}

impl HttpMethod {
    fn named(name: &str) -> Option<Self> {
        match name {
            "get" => Some(Self::Get),
            "post" => Some(Self::Post),
            "put" => Some(Self::Put),
            "patch" => Some(Self::Patch),
            "delete" => Some(Self::Delete),
            _ => None,
        }
    }

    /// The lower-case name, which is also the name of the router's endpoint constructor.
    pub fn name(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Post => "post",
            Self::Put => "put",
            Self::Patch => "patch",
            Self::Delete => "delete",
        }
    }

    /// The name of the matching associated constant of `http::Method`.
    pub fn constant(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

/// A parsed `#[get("/path", when(..))]` style attribute.
pub struct RouteAttr {
    /// The method the attribute names.
    pub method: HttpMethod,
    /// The path relative to the controller prefix; empty when omitted.
    pub path: LitStr,
    /// Boolean expressions over the method's parameters, all of which must hold.
    pub guards: Vec<Expr>,
    /// The `when` clause as written, for spotting duplicates.
    pub when_text: String,
}

impl RouteAttr {
    /// Parses `attr` when it is a route attribute, `None` for any other attribute.
    pub fn from_attribute(attr: &Attribute) -> Option<syn::Result<Self>> {
        let method = attr
            .path()
            .get_ident()
            .and_then(|ident| HttpMethod::named(&ident.to_string()))?;

        Some(match &attr.meta {
            Meta::Path(_) => Ok(Self::bare(method)),
            Meta::List(list) => {
                list.parse_args_with(|input: ParseStream| Self::parse_args(method, input))
            }
            Meta::NameValue(pair) => Err(syn::Error::new_spanned(
                pair,
                "expected `#[method(\"/path\")]`",
            )),
        })
    }

    fn bare(method: HttpMethod) -> Self {
        Self {
            method,
            path: LitStr::new("", Span::call_site()),
            guards: Vec::new(),
            when_text: String::new(),
        }
    }

    fn parse_args(method: HttpMethod, input: ParseStream) -> syn::Result<Self> {
        let path = if input.peek(LitStr) {
            let path = input.parse()?;

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }

            path
        } else {
            LitStr::new("", Span::call_site())
        };
        let mut guards = Vec::new();
        let mut when_text = String::new();

        if !input.is_empty() {
            let keyword: Ident = input.parse()?;

            if keyword != "when" {
                return Err(syn::Error::new_spanned(keyword, "expected `when(..)`"));
            }

            let content;

            parenthesized!(content in input);
            when_text = content.fork().parse::<TokenStream>()?.to_string();
            guards = Punctuated::<Expr, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect();

            if guards.is_empty() {
                return Err(syn::Error::new_spanned(
                    keyword,
                    "`when(..)` needs a condition",
                ));
            }
        }

        if !input.is_empty() {
            return Err(input.error("unexpected tokens after the route attribute arguments"));
        }

        Ok(Self {
            method,
            path,
            guards,
            when_text,
        })
    }
}

#[cfg(test)]
mod tests;
