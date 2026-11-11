use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Ident};

use super::{
    dispatcher::{self, Naming},
    group,
    model::{Context, Handler},
    partition::RoutePlan,
};

/// What the expansion adds around the handlers: endpoint items, and the statements that register
/// each route in `routes`.
pub struct Generated {
    /// Items added to the controller's impl block.
    pub members: Vec<TokenStream>,
    /// Items emitted next to the impl block or module, such as the generated endpoints.
    pub extra: Vec<TokenStream>,
    /// One statement per route, each shadowing `__router` with the route added.
    pub routes: Vec<TokenStream>,
}

/// Generates the endpoints of every route in `plans`, and the statements that register them.
///
/// Registration is a chain of shadowing `let`s rather than one builder expression, so the
/// `#[cfg(..)]` of a handler can gate its endpoint, and the route when every endpoint is gated.
pub fn generate(
    ctx: &Context,
    handlers: &[Handler],
    plans: &[RoutePlan],
) -> syn::Result<Generated> {
    let krate = ctx.krate;
    let mut generated = Generated {
        members: Vec::new(),
        extra: Vec::new(),
        routes: Vec::new(),
    };

    for route in plans {
        let pattern = &route.pattern;
        let start = match &ctx.base {
            Some(base) => quote!(#krate::router::Route::under(#base, #pattern)),
            None => quote!(#krate::router::Route::new(#pattern)),
        };
        let mut endpoints = Vec::new();
        let mut gates = Vec::new();

        for partition in &route.partitions {
            let dispatcher = dispatcher::dispatcher(
                ctx,
                handlers,
                partition,
                Naming::derived(generated.extra.len()),
            )?;
            let cfgs = &dispatcher.cfgs;
            let expr = &dispatcher.expr;

            endpoints.push(quote! {
                #(#cfgs)*
                let __route = __route.endpoint(#expr);
            });
            gates.push(dispatcher.cfgs.clone());
            generated.extra.push(dispatcher.items);
        }

        let gate = route_gate(&gates)?;

        generated.routes.push(quote! {
            #gate
            let __router = {
                let __route = #start?;

                #(#endpoints)*

                __router.route(__route)
            };
        });
    }

    Ok(generated)
}

/// The `#[cfg(..)]` of a whole route: present only when every endpoint is gated, so a route whose
/// endpoints are all compiled out is not registered empty.
fn route_gate(gates: &[Vec<Attribute>]) -> syn::Result<Option<TokenStream>> {
    if gates.is_empty() || gates.iter().any(Vec::is_empty) {
        return Ok(None);
    }

    let mut alternatives = Vec::new();

    for cfgs in gates {
        let predicates = cfgs
            .iter()
            .map(Attribute::parse_args::<TokenStream>)
            .collect::<syn::Result<Vec<_>>>()?;

        alternatives.push(quote!(all(#(#predicates),*)));
    }

    Ok(Some(quote!(#[cfg(any(#(#alternatives),*))])))
}

/// The function that builds the router from the `routes` statements: `routes`, or a group's
/// hidden builder.
pub fn routes_fn(ctx: &Context, routes: &[TokenStream], group: Option<&Ident>) -> TokenStream {
    let krate = ctx.krate;
    let receiver = ctx
        .owner
        .as_ref()
        .map(|_| quote!(self: ::std::sync::Arc<Self>,));
    let signature = match group {
        Some(group) => {
            let name = group::group_fn(group);

            quote! {
                /// Builds the router of this group, under the controller's path.
                #[doc(hidden)]
                pub(crate) fn #name
            }
        }
        None => quote! {
            /// Builds the router of these routes from their route attributes.
            pub fn routes
        },
    };

    quote! {
        #signature(
            #receiver
        ) -> ::std::result::Result<
            #krate::router::Router<impl #krate::router::RouteList>,
            #krate::router::PatternError,
        > {
            let __router = #krate::router::Router::new();

            #(#routes)*

            ::std::result::Result::Ok(__router)
        }
    }
}
