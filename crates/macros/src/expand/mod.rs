use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Attribute, Error, FnArg, Ident, ImplItem, Item, ItemFn, ItemImpl, ItemMod, Lifetime, Signature,
    Visibility, visit_mut::VisitMut,
};

use crate::{
    args::ControllerArgs,
    guard::{analyze, bindings},
    path::join,
    route::{HttpMethod, RouteAttr, split_method},
};

mod dispatcher;
mod model;
mod partition;

pub use model::{Handler, Param};

use self::{
    dispatcher::Naming,
    model::{Context, Owner, Variant},
    partition::{RoutePlan, plan},
};

/// Expands `#[controller(args)]` applied to an impl block `item`.
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
    let mut errors = Vec::new();

    for member in &mut item.items {
        if let ImplItem::Fn(method) = member {
            collect(
                &mut method.attrs,
                &mut method.sig,
                true,
                &args,
                &mut handlers,
                &mut variants,
                &mut errors,
            )?;
        }
    }

    let plans = plan(&variants)?;
    let ctx = Context {
        krate: &args.krate,
        owner: Some(Owner {
            self_ty: &item.self_ty,
            generics: &item.generics,
        }),
    };
    let generated = generate(&ctx, &handlers, &plans)?;
    let routes = routes_fn(&ctx, &generated.routes);

    for member in generated.members {
        item.items.push(syn::parse2(member)?);
    }

    item.items.push(syn::parse2(routes)?);

    let extra = generated.extra;
    let errors = errors.iter().map(Error::to_compile_error);

    Ok(quote!(#item #(#extra)* #(#errors)*))
}

/// Expands `#[routes(args)]` applied to an inline module `item` of free functions.
pub fn routes(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let args: ControllerArgs = syn::parse2(args)?;
    let mut module: ItemMod = syn::parse2(item)?;
    let Some((_, items)) = &mut module.content else {
        return Err(Error::new_spanned(
            &module.ident,
            "#[routes] applies to an inline module: `mod name { .. }`",
        ));
    };
    let mut handlers = Vec::new();
    let mut variants = Vec::new();
    let mut errors = Vec::new();

    for member in items.iter_mut() {
        if let Item::Fn(function) = member {
            collect(
                &mut function.attrs,
                &mut function.sig,
                false,
                &args,
                &mut handlers,
                &mut variants,
                &mut errors,
            )?;
        }
    }

    let plans = plan(&variants)?;
    let ctx = Context {
        krate: &args.krate,
        owner: None,
    };
    let generated = generate(&ctx, &handlers, &plans)?;
    let routes = routes_fn(&ctx, &generated.routes);

    for extra in generated.extra {
        items.extend(syn::parse2::<syn::File>(extra)?.items);
    }

    items.push(syn::parse2(routes)?);

    for error in &errors {
        items.extend(syn::parse2::<syn::File>(error.to_compile_error())?.items);
    }

    Ok(quote!(#module))
}

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
    let krate: syn::Path = syn::parse_quote!(::fast_http);

    check_signature(&function.sig, false)?;

    let name = function.sig.ident.clone();
    let inner = format_ident!("__fast_http_handler_{}", name);
    let params = params(&function.sig);

    allow_guard_only_params(&mut function.sig, &route.guards, &params)?;

    function.sig.ident = inner.clone();

    let vis = std::mem::replace(&mut function.vis, Visibility::Inherited);
    let pattern = join(None, &route.path)?;
    let handlers = vec![Handler {
        name: inner,
        receiver: false,
        params,
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

        impl #name {
            /// The path of this function's route attribute.
            #vis const PATH: &'static str = #pattern;
        }

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

/// What `#[controller]` and `#[routes]` emit when they cannot expand: the item as written, minus
/// the route attributes they would have consumed, followed by the error.
///
/// Emitting only the error would remove every function from the editor's view, so a half-typed
/// guard would blank out type hints and completion for the whole controller.
pub fn recover(item: TokenStream, error: syn::Error) -> TokenStream {
    let error = error.into_compile_error();

    if let Ok(mut parsed) = syn::parse2::<ItemImpl>(item.clone()) {
        for member in &mut parsed.items {
            if let ImplItem::Fn(method) = member {
                method.attrs.retain(|attr| !RouteAttr::is_route(attr));
            }
        }

        return quote!(#parsed #error);
    }

    if let Ok(mut parsed) = syn::parse2::<ItemMod>(item.clone()) {
        if let Some((_, items)) = &mut parsed.content {
            for member in items.iter_mut() {
                if let Item::Fn(function) = member {
                    function.attrs.retain(|attr| !RouteAttr::is_route(attr));
                }
            }
        }

        return quote!(#parsed #error);
    }

    quote!(#item #error)
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

    for route in plans {
        let pattern = &route.pattern;
        let mut endpoints = Vec::new();

        for partition in &route.partitions {
            let dispatcher = dispatcher::dispatcher(
                ctx,
                handlers,
                partition,
                Naming::derived(generated.extra.len()),
            )?;

            generated.extra.push(dispatcher.items);
            endpoints.push(dispatcher.expr);
        }

        generated
            .routes
            .push(quote!(.route(#krate::router::Route::new(#pattern)? #(.endpoint(#endpoints))*)));
    }

    Ok(generated)
}

fn routes_fn(ctx: &Context, routes: &[TokenStream]) -> TokenStream {
    let krate = ctx.krate;
    let receiver = ctx
        .owner
        .as_ref()
        .map(|_| quote!(self: ::std::sync::Arc<Self>,));

    quote! {
        /// Builds the router of these routes from their route attributes.
        pub fn routes(
            #receiver
        ) -> ::std::result::Result<
            #krate::router::Router<impl #krate::router::RouteList>,
            #krate::router::PatternError,
        > {
            ::std::result::Result::Ok(#krate::router::Router::new() #(#routes)*)
        }
    }
}

fn collect(
    attrs: &mut Vec<Attribute>,
    sig: &mut Signature,
    allow_receiver: bool,
    args: &ControllerArgs,
    handlers: &mut Vec<Handler>,
    variants: &mut Vec<Variant>,
    errors: &mut Vec<Error>,
) -> syn::Result<()> {
    let mut kept = Vec::new();
    let mut routes = Vec::new();

    for attr in std::mem::take(attrs) {
        match RouteAttr::from_attribute(&attr) {
            Some(parsed) => {
                let mut parsed = parsed?;

                errors.append(&mut parsed.errors);
                routes.push(parsed);
            }
            None => kept.push(attr),
        }
    }

    *attrs = kept;

    if routes.is_empty() {
        return Ok(());
    }

    check_signature(sig, allow_receiver)?;

    let params = params(sig);

    let guards: Vec<_> = routes.iter().flat_map(|route| &route.guards).collect();

    allow_guard_only_params(
        sig,
        guards.into_iter().cloned().collect::<Vec<_>>().as_slice(),
        &params,
    )?;

    handlers.push(Handler {
        name: sig.ident.clone(),
        receiver: matches!(sig.inputs.first(), Some(FnArg::Receiver(_))),
        params,
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
/// exists only to feed a guard does not warn in the function that never touches it.
fn allow_guard_only_params(
    sig: &mut Signature,
    guards: &[syn::Expr],
    params: &[Param],
) -> syn::Result<()> {
    let mut used = BTreeSet::new();

    for guard in guards {
        used.extend(analyze(guard, params)?.uses);
    }

    let typed = sig.inputs.iter_mut().filter_map(|arg| match arg {
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

/// Rewrites the lifetimes a function declares to `'_`, so a generated endpoint can name the
/// argument types without declaring them: each extraction borrows from the request anyway.
struct Elide<'a>(&'a [Ident]);

impl VisitMut for Elide<'_> {
    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        if self.0.contains(&lifetime.ident) {
            *lifetime = Lifetime::new("'_", lifetime.span());
        }
    }
}

fn params(sig: &Signature) -> Vec<Param> {
    let declared: Vec<Ident> = sig
        .generics
        .lifetimes()
        .map(|param| param.lifetime.ident.clone())
        .collect();

    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(typed) => Some(Param {
                pat: (*typed.pat).clone(),
                ty: {
                    let mut ty = (*typed.ty).clone();

                    Elide(&declared).visit_type_mut(&mut ty);

                    ty
                },
                bindings: bindings(&typed.pat),
            }),
            FnArg::Receiver(_) => None,
        })
        .collect()
}

fn check_signature(sig: &Signature, allow_receiver: bool) -> syn::Result<()> {
    if sig.asyncness.is_none() {
        return Err(Error::new_spanned(
            sig.fn_token,
            "route functions must be `async fn`",
        ));
    }

    if sig.generics.type_params().next().is_some() || sig.generics.const_params().next().is_some() {
        return Err(Error::new_spanned(
            &sig.generics,
            "route functions cannot have type or const parameters; lifetimes are fine",
        ));
    }

    match sig.inputs.first() {
        Some(FnArg::Receiver(receiver)) if !allow_receiver => Err(Error::new_spanned(
            receiver,
            "a free function has no `self`; use #[controller] on an impl block for state",
        )),
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
