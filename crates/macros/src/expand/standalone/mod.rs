use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Error, ItemFn, Visibility};

use super::{
    allow_guard_only_params,
    cfg::Gate,
    check_signature,
    dispatcher::{self, Naming},
    model::{Context, Handler, Variant},
    params,
    partition::plan,
};
use crate::{
    path::join,
    route::{HttpMethod, RouteAttr, split_method},
};

/// Expands `#[get(..)]` and its siblings applied to a free function `item`.
///
/// The function keeps its body under a hidden name. Its own name becomes a unit struct that is
/// the endpoint, with a `route()` helper that builds the route at the attribute's path.
pub fn standalone(
    method: HttpMethod,
    args: TokenStream,
    item: TokenStream,
) -> syn::Result<TokenStream> {
    let route = RouteAttr::from_args(method, args)?;
    let errors: Vec<_> = route.errors.iter().map(Error::to_compile_error).collect();
    let mut function: ItemFn = syn::parse2(item)?;
    let krate = route
        .krate
        .clone()
        .unwrap_or_else(|| syn::parse_quote!(::fast_http));

    check_signature(&function.sig, false)?;

    let name = function.sig.ident.clone();
    let inner = format_ident!("__fast_http_handler_{}", name);
    let params = params(&function.sig);

    allow_guard_only_params(&mut function.sig, &route.guards, &params)?;

    function.sig.ident = inner.clone();

    let vis = std::mem::replace(&mut function.vis, Visibility::Inherited);
    let pattern = join(None, &route.path)?;
    let gate = Gate::of(&function.attrs)?;
    let cfg = gate.attr();
    let handlers = vec![Handler {
        name: inner,
        receiver: false,
        params,
        gate,
    }];
    let variants = vec![Variant {
        handler: 0,
        method,
        pattern: pattern.clone(),
        guards: route.guards,
        when_text: route.when_text,
    }];
    let plans = plan(&variants)?;
    let ctx = Context {
        krate: &krate,
        owner: None,
        base: None,
    };
    let partition = &plans[0].partitions[0];
    let generated = dispatcher::dispatcher(
        &ctx,
        &handlers,
        partition,
        Naming {
            name: Some(name.clone()),
            vis: quote!(#vis),
            index: 0,
        },
    )?;
    let items = generated.items;

    Ok(quote! {
        #[doc(hidden)]
        #function

        #items

        #(#errors)*

        #cfg
        impl #name {
            /// The path of this function's route attribute.
            #vis const PATH: &'static str = #pattern;
        }

        #cfg
        impl #krate::router::IntoRoute for #name {
            type Set = (#name,);
            type Flow = #krate::core::Identity;

            fn into_route(self) -> #krate::router::Route<(#name,), #krate::core::Identity> {
                #krate::router::route::attributed(Self::PATH).endpoint(self)
            }
        }
    })
}

/// Expands the general `#[route(METHOD, "/path", when(..))]` applied to a free function `item`.
pub fn standalone_route(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let (method, rest) = syn::parse::Parser::parse2(split_method, args)?;

    standalone(method, rest, item)
}
