use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Error, Ident, ItemStruct, LitStr};

use crate::{args::ControllerArgs, path::join};

/// The hidden function an impl block of group `group` builds its router with, named so that a
/// listed group without a block, or a block nobody lists, points at the group's name.
pub fn group_fn(group: &Ident) -> Ident {
    format_ident!("__fast_http_group_{}", group, span = group.span())
}

/// The base path a group's routes are joined under: the controller's [`PATH`] at startup.
///
/// [`PATH`]: https://docs.rs/fast-http-router/latest/fast_http_router/controller/trait.Controller.html
pub fn base(args: &ControllerArgs) -> Option<TokenStream> {
    let krate = &args.krate;

    args.group
        .as_ref()
        .map(|_| quote!(<Self as #krate::router::Controller>::PATH))
}

/// Expands `#[controller("/path", groups(a, b))]` on a struct: implements `Controller` with the
/// path, and adds `routes(self: Arc<Self>)`, which merges the routers of the listed groups.
pub fn controller_struct(args: &ControllerArgs, item: &ItemStruct) -> syn::Result<TokenStream> {
    if let Some(group) = &args.group {
        return Err(Error::new_spanned(
            group,
            "`group = name` belongs on an impl block; the struct lists its groups with `groups(..)`",
        ));
    }

    let Some((last, rest)) = args.groups.split_last() else {
        return Err(Error::new_spanned(
            &item.ident,
            "a controller struct lists the impl blocks that hold its routes: `#[controller(\"/path\", groups(name, ..))]`",
        ));
    };

    for (index, group) in args.groups.iter().enumerate() {
        if args.groups[..index].contains(group) {
            return Err(Error::new_spanned(group, "this group is listed twice"));
        }
    }

    let krate = &args.krate;
    let name = &item.ident;
    let path = join(args.prefix.as_ref(), &LitStr::new("", Span::call_site()))?;
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();
    let shared: Vec<_> = rest.iter().map(group_fn).collect();
    let last = group_fn(last);

    Ok(quote! {
        #item

        impl #impl_generics #krate::router::Controller for #name #type_generics #where_clause {
            const PATH: &'static str = #path;
        }

        impl #impl_generics #name #type_generics #where_clause {
            /// Builds the router of the controller: the routes of every listed group, under its
            /// path.
            pub fn routes(
                self: ::std::sync::Arc<Self>,
            ) -> ::std::result::Result<
                #krate::router::Router<impl #krate::router::RouteList>,
                #krate::router::PatternError,
            > {
                ::std::result::Result::Ok(
                    #krate::router::Router::new()
                        #(.merge(Self::#shared(::std::sync::Arc::clone(&self))?))*
                        .merge(Self::#last(self)?),
                )
            }
        }
    })
}

#[cfg(test)]
mod tests;
