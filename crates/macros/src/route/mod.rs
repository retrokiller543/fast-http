use proc_macro2::{Span, TokenStream, TokenTree};
use quote::quote;
use syn::{Attribute, Expr, Ident, LitStr, Meta, Token, parenthesized, parse::ParseStream};

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
    /// Syntax errors in guards, which are reported next to a full expansion instead of replacing
    /// it, so the editor keeps its view of everything else.
    pub errors: Vec<syn::Error>,
}

impl RouteAttr {
    /// Whether `attr` is a route attribute, without parsing its arguments.
    pub fn is_route(attr: &Attribute) -> bool {
        attr.path().get_ident().is_some_and(|ident| {
            ident == "route" || HttpMethod::named(&ident.to_string()).is_some()
        })
    }

    /// Parses `attr` when it is a route attribute, `None` for any other attribute.
    pub fn from_attribute(attr: &Attribute) -> Option<syn::Result<Self>> {
        if attr.path().is_ident("route") {
            return Some(match &attr.meta {
                Meta::List(list) => list.parse_args_with(Self::parse_route_args),
                other => Err(syn::Error::new_spanned(
                    other,
                    "expected `#[route(METHOD, \"/path\")]`",
                )),
            });
        }

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
            errors: Vec::new(),
        }
    }

    /// Parses the arguments of the general form, `METHOD, "/path", when(..)`.
    pub fn parse_route_args(input: ParseStream) -> syn::Result<Self> {
        let (method, rest) = split_method(input)?;

        syn::parse::Parser::parse2(|input: ParseStream| Self::parse_args(method, input), rest)
    }

    /// Parses the arguments of a standalone `#[get(..)]` style attribute on a free function.
    pub fn from_args(method: HttpMethod, args: TokenStream) -> syn::Result<Self> {
        if args.is_empty() {
            return Ok(Self::bare(method));
        }

        syn::parse::Parser::parse2(|input: ParseStream| Self::parse_args(method, input), args)
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
        let mut errors = Vec::new();

        if !input.is_empty() {
            let keyword: Ident = input.parse()?;

            if keyword != "when" {
                return Err(syn::Error::new_spanned(keyword, "expected `when(..)`"));
            }

            let content;

            parenthesized!(content in input);
            when_text = content.fork().parse::<TokenStream>()?.to_string();
            guards = parse_guards(&content, &mut errors);

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
            errors,
        })
    }
}

/// Reads the leading `METHOD` of `#[route(METHOD, ..)]`, in any case, and returns the rest.
pub fn split_method(input: ParseStream) -> syn::Result<(HttpMethod, TokenStream)> {
    let ident: Ident = input.parse()?;
    let method = HttpMethod::named(&ident.to_string().to_ascii_lowercase()).ok_or_else(|| {
        syn::Error::new_spanned(&ident, "expected GET, POST, PUT, PATCH or DELETE")
    })?;

    if !input.is_empty() {
        input.parse::<Token![,]>()?;
    }

    Ok((method, input.parse()?))
}

/// Parses the guards of a `when(..)`, one expression at a time.
///
/// A guard that does not parse is repaired when the fix is obvious, or replaced by `false`, and
/// its error is recorded, so one half-typed guard does not take the other guards, or the editor's
/// view of the whole controller, down with it.
fn parse_guards(content: ParseStream, errors: &mut Vec<syn::Error>) -> Vec<Expr> {
    let mut guards = Vec::new();

    while !content.is_empty() {
        let fork = content.fork();
        let parsed = fork.parse::<Expr>().is_ok() && (fork.is_empty() || fork.peek(Token![,]));

        match parsed.then(|| content.parse::<Expr>()) {
            Some(Ok(expr)) => guards.push(expr),
            _ => {
                let chunk = take_chunk(content);

                if !chunk.is_empty() {
                    let (expr, error) = lenient(chunk);

                    errors.push(error);
                    guards.push(expr);
                }
            }
        }

        if content.peek(Token![,]) {
            let _ = content.parse::<Token![,]>();
        }
    }

    guards
}

fn take_chunk(content: ParseStream) -> TokenStream {
    let mut chunk = TokenStream::new();

    while !content.is_empty() && !content.peek(Token![,]) {
        match content.parse::<TokenTree>() {
            Ok(token) => chunk.extend([token]),
            Err(_) => break,
        }
    }

    chunk
}

/// Turns tokens that are not an expression into one, and the error that says why they were not.
fn lenient(chunk: TokenStream) -> (Expr, syn::Error) {
    let error = syn::parse2::<Expr>(chunk.clone()).err().unwrap_or_else(|| {
        syn::Error::new_spanned(&chunk, "this guard is not a single expression")
    });

    if let Some(repaired) = repair(&chunk)
        && let Ok(expr) = syn::parse2::<Expr>(repaired)
    {
        return (expr, error);
    }

    (syn::parse_quote!(false), error)
}

/// Completes the usual unfinished tail of a guard so that the rest of it still has meaning: a
/// trailing `.` gets a placeholder member, a trailing operator gets `false`.
fn repair(chunk: &TokenStream) -> Option<TokenStream> {
    let last = chunk.clone().into_iter().last()?;
    let TokenTree::Punct(punct) = last else {
        return None;
    };
    let tail = match punct.as_char() {
        '.' => quote!(__incomplete),
        '&' | '|' | '=' | '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '^' => quote!(false),
        _ => return None,
    };

    Some(quote!(#chunk #tail))
}

#[cfg(test)]
mod tests;
