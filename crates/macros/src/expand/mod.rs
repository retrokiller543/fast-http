use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Error, FnArg, ImplItem, ImplItemFn, ItemImpl};

use crate::{
    args::ControllerArgs,
    guard::{analyze, bindings},
    path::join,
    route::RouteAttr,
};

mod dispatcher;
mod model;
mod partition;
mod simple;

pub use model::{Handler, Param};

use self::{
    model::{Context, Variant},
    partition::{RoutePlan, plan},
};

/// Expands `#[controller(args)]` applied to `item`.
pub fn controller(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let args: ControllerArgs = syn::parse2(args)?;
    let mut item: ItemImpl = syn::parse2(item)?;

    if let Some((_, path, _)) = &item.trait_ {
        return Err(Error::new_spanned(
            path,
            "#[controller] applies to an inherent impl block",
        ));
    }

    let mut handlers = Vec::new();
    let mut variants = Vec::new();

    for member in &mut item.items {
        if let ImplItem::Fn(method) = member {
            collect(method, &args, &mut handlers, &mut variants)?;
        }
    }

    let plans = plan(&variants)?;
    let ctx = Context {
        krate: &args.krate,
        self_ty: &item.self_ty,
        generics: &item.generics,
    };
    let generated = generate(&ctx, &handlers, &plans)?;
    let routes = routes_fn(&ctx, &generated.routes);

    for member in generated.members {
        item.items.push(syn::parse2(member)?);
    }

    item.items.push(syn::parse2(routes)?);

    let extra = generated.extra;

    Ok(quote!(#item #(#extra)*))
}

struct Generated {
    members: Vec<TokenStream>,
    extra: Vec<TokenStream>,
    routes: Vec<TokenStream>,
}

fn generate(ctx: &Context, handlers: &[Handler], plans: &[RoutePlan]) -> syn::Result<Generated> {
    let krate = ctx.krate;
    let mut generated = Generated {
        members: Vec::new(),
        extra: Vec::new(),
        routes: Vec::new(),
    };
    let mut wrapped = BTreeSet::new();

    for route in plans {
        let pattern = &route.pattern;
        let mut endpoints = Vec::new();

        for partition in &route.partitions {
            if partition.is_simple() {
                let index = partition.variants[0].handler;
                let handler = &handlers[index];

                if handler.receiver && wrapped.insert(index) {
                    generated.members.push(simple::wrapper_fn(handler));
                }

                endpoints.push(simple::endpoint(ctx, handler, partition.method));
            } else {
                let dispatcher = dispatcher::dispatcher(ctx, handlers, partition)?;

                generated.extra.push(dispatcher.items);
                endpoints.push(dispatcher.expr);
            }
        }

        generated
            .routes
            .push(quote!(.route(#krate::router::Route::new(#pattern)? #(.endpoint(#endpoints))*)));
    }

    Ok(generated)
}

fn routes_fn(ctx: &Context, routes: &[TokenStream]) -> TokenStream {
    let krate = ctx.krate;

    quote! {
        /// Builds the router of this controller from its route attributes.
        pub fn routes(
            self: ::std::sync::Arc<Self>,
        ) -> ::std::result::Result<
            #krate::router::Router<impl #krate::router::RouteList>,
            #krate::router::PatternError,
        > {
            ::std::result::Result::Ok(#krate::router::Router::new() #(#routes)*)
        }
    }
}

fn collect(
    method: &mut ImplItemFn,
    args: &ControllerArgs,
    handlers: &mut Vec<Handler>,
    variants: &mut Vec<Variant>,
) -> syn::Result<()> {
    let mut attrs = Vec::new();
    let mut routes = Vec::new();

    for attr in std::mem::take(&mut method.attrs) {
        match RouteAttr::from_attribute(&attr) {
            Some(parsed) => routes.push(parsed?),
            None => attrs.push(attr),
        }
    }

    method.attrs = attrs;

    if routes.is_empty() {
        return Ok(());
    }

    check_signature(method)?;

    let params = params(method);

    allow_guard_only_params(method, &routes, &params)?;

    handlers.push(Handler {
        name: method.sig.ident.clone(),
        receiver: matches!(method.sig.inputs.first(), Some(FnArg::Receiver(_))),
        params,
        output: method.sig.output.clone(),
    });

    for route in routes {
        variants.push(Variant {
            handler: handlers.len() - 1,
            method: route.method,
            pattern: join(args.prefix.as_ref(), &route.path)?,
            guards: route.guards,
            when_text: route.when_text,
        });
    }

    Ok(())
}

/// Marks the parameters that guards read with `#[allow(unused_variables)]`, so a parameter that
/// exists only to feed a guard does not warn in the method that never touches it.
fn allow_guard_only_params(
    method: &mut ImplItemFn,
    routes: &[RouteAttr],
    params: &[Param],
) -> syn::Result<()> {
    let mut used = BTreeSet::new();

    for guard in routes.iter().flat_map(|route| &route.guards) {
        used.extend(analyze(guard, params)?.uses);
    }

    let typed = method.sig.inputs.iter_mut().filter_map(|arg| match arg {
        FnArg::Typed(typed) => Some(typed),
        FnArg::Receiver(_) => None,
    });

    for (index, param) in typed.enumerate() {
        if used.contains(&index) {
            param
                .attrs
                .push(syn::parse_quote!(#[allow(unused_variables)]));
        }
    }

    Ok(())
}

fn params(method: &ImplItemFn) -> Vec<Param> {
    method
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(typed) => Some(Param {
                pat: (*typed.pat).clone(),
                ty: (*typed.ty).clone(),
                bindings: bindings(&typed.pat),
            }),
            FnArg::Receiver(_) => None,
        })
        .collect()
}

fn check_signature(method: &ImplItemFn) -> syn::Result<()> {
    let sig = &method.sig;

    if sig.asyncness.is_none() {
        return Err(Error::new_spanned(
            sig.fn_token,
            "route methods must be `async fn`",
        ));
    }

    if !sig.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &sig.generics,
            "route methods cannot be generic",
        ));
    }

    match sig.inputs.first() {
        Some(FnArg::Receiver(receiver))
            if receiver.reference.is_none()
                || receiver.mutability.is_some()
                || receiver.colon_token.is_some() =>
        {
            Err(Error::new_spanned(
                receiver,
                "route methods take `&self` or no receiver",
            ))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
