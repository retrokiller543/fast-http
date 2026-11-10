use syn::{
    LitStr, Path, Token,
    parse::{Parse, ParseStream},
};

/// Arguments of `#[controller(..)]`.
pub struct ControllerArgs {
    /// Path prefix shared by every route of the controller.
    pub prefix: Option<LitStr>,
    /// Crate that provides the `router` and `core` modules the expansion refers to.
    pub krate: Path,
}

impl Parse for ControllerArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut prefix = None;
        let mut krate = syn::parse_quote!(::fast_http);

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
            } else {
                return Err(input.error("expected a path prefix string or `crate = path`"));
            }

            if input.is_empty() {
                break;
            }

            input.parse::<Token![,]>()?;
        }

        Ok(Self { prefix, krate })
    }
}

#[cfg(test)]
mod tests;
