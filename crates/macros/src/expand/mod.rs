use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute, Error, FnArg, Ident, ImplItem, Item, ItemImpl, ItemMod, Lifetime, Signature,
    visit_mut::VisitMut,
};

use crate::{
    args::ControllerArgs,
    guard::{analyze, bindings},
    path::join,
    route::RouteAttr,
};

mod cfg;
mod dispatcher;
mod group;
mod model;
mod partition;
mod register;
mod standalone;

pub use model::{Handler, Param};
pub use standalone::{standalone, standalone_route};

use self::{
    model::{Context, Owner, Variant},
    partition::plan,
    register::{generate, routes_fn},
};

/// Expands `#[controller(args)]` applied to an impl block `item`.
pub fn controller(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let args: ControllerArgs = syn::parse2(args)?;
    let mut item = match syn::parse2::<Item>(item)? {
        Item::Impl(item) => item,
        Item::Struct(item) => return group::controller_struct(&args, &item),
        other => {
            return Err(Error::new_spanned(
                other,
                "#[controller] applies to an inherent impl block, or to a struct with `groups(..)`",
            ));
        }
    };

    if let Some(first) = args.groups.first() {
        return Err(Error::new_spanned(
            first,
            "`groups(..)` belongs on the controller struct; an impl block names its own with `group = name`",
        ));
    }

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
        base: group::base(&args),
    };
    let generated = generate(&ctx, &handlers, &plans)?;
    let routes = routes_fn(&ctx, &generated.routes, args.group.as_ref());

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

    if let Some(key) = args.group.as_ref().or(args.groups.first()) {
        return Err(Error::new_spanned(
            key,
            "groups belong to a #[controller] struct and its impl blocks",
        ));
    }
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
        base: None,
    };
    let generated = generate(&ctx, &handlers, &plans)?;
    let routes = routes_fn(&ctx, &generated.routes, None);

    for extra in generated.extra {
        items.extend(syn::parse2::<syn::File>(extra)?.items);
    }

    items.push(syn::parse2(routes)?);

    for error in &errors {
        items.extend(syn::parse2::<syn::File>(error.to_compile_error())?.items);
    }

    Ok(quote!(#module))
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

                if let Some(krate) = &parsed.krate {
                    return Err(Error::new_spanned(
                        krate,
                        "`crate = path` goes on the #[controller] or #[routes] attribute, which expands these routes",
                    ));
                }

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
        gate: cfg::Gate::of(attrs)?,
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
