use syn::{
    Ident, LitStr, Path, Token, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

/// Arguments of `#[controller(..)]`.
pub struct ControllerArgs {
    /// Path prefix shared by every route of the controller.
    pub prefix: Option<LitStr>,
    /// Crate that provides the `router` and `core` modules the expansion refers to.
    pub krate: Path,
    /// `groups(a, b)` on a controller struct: the impl blocks whose routes it merges.
    pub groups: Vec<Ident>,
    /// `group = name` on an impl block: the group of a controller struct it belongs to.
    pub group: Option<Ident>,
}

impl Parse for ControllerArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut prefix = None;
        let mut krate = syn::parse_quote!(::fast_http);
        let mut groups = Vec::new();
        let mut group = None;

        while !input.is_empty() {
            if input.peek(LitStr) {
                if prefix.is_some() {
                    return Err(input.error("the path prefix is given twice"));
                }

                prefix = Some(input.parse()?);
            } else if input.peek(Token![crate]) {
                input.parse::<Token![crate]>()?;
                input.parse::<Token![=]>()?;
                krate = input.parse()?;
            } else if input.peek(Ident) {
                let key: Ident = input.parse()?;

                if key == "groups" {
                    let content;

                    parenthesized!(content in input);
                    groups = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?
                        .into_iter()
                        .collect();
                } else if key == "group" {
                    input.parse::<Token![=]>()?;
                    group = Some(input.parse()?);
                } else {
                    return Err(syn::Error::new_spanned(
                        key,
                        "expected a path prefix string, `groups(..)`, `group = name` or `crate = path`",
                    ));
                }
            } else {
                return Err(input.error(
                    "expected a path prefix string, `groups(..)`, `group = name` or `crate = path`",
                ));
            }

            if input.is_empty() {
                break;
            }

            input.parse::<Token![,]>()?;
        }

        Ok(Self {
            prefix,
            krate,
            groups,
            group,
        })
    }
}

#[cfg(test)]
mod tests;
