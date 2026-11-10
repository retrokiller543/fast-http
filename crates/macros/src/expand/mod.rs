use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{Error, FnArg, ImplItem, ImplItemFn, ItemImpl, LitStr, Path};

use crate::{
    args::ControllerArgs,
    path::join,
    route::{HttpMethod, RouteAttr},
    when::Condition,
};

mod group;

use self::group::{Endpoint, Group, group};

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

    let mut endpoints = Vec::new();
    let mut wrappers = Vec::new();

    for member in &mut item.items {
        if let ImplItem::Fn(method) = member {
            collect(method, &args, &mut endpoints, &mut wrappers)?;
        }
    }

    let groups = group(endpoints)?;
    let routes = routes_fn(&args.krate, &groups);

    for wrapper in wrappers {
        item.items.push(syn::parse2(wrapper)?);
    }

    item.items.push(syn::parse2(routes)?);

    Ok(item.into_token_stream())
}

fn collect(
    method: &mut ImplItemFn,
    args: &ControllerArgs,
    endpoints: &mut Vec<Endpoint>,
    wrappers: &mut Vec<TokenStream>,
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

    let handler = handler(method, &args.krate, wrappers);

    for route in routes {
        endpoints.push(Endpoint {
            method: route.method,
            pattern: join(args.prefix.as_ref(), &route.path)?,
            when: route.when,
            when_text: route.when_text,
            handler: handler.clone(),
        });
    }

    Ok(())
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

fn has_receiver(method: &ImplItemFn) -> bool {
    matches!(method.sig.inputs.first(), Some(FnArg::Receiver(_)))
}

fn handler(method: &ImplItemFn, krate: &Path, wrappers: &mut Vec<TokenStream>) -> TokenStream {
    let name = &method.sig.ident;

    if !has_receiver(method) {
        return quote!(Self::#name);
    }

    let wrapper = format_ident!("__fast_http_{}", name);

    if !wrappers
        .iter()
        .any(|existing| existing.to_string().contains(&wrapper.to_string()))
    {
        wrappers.push(wrapper_fn(method, &wrapper));
    }

    quote!(#krate::router::bind(::std::sync::Arc::clone(&self), Self::#wrapper))
}

fn wrapper_fn(method: &ImplItemFn, wrapper: &syn::Ident) -> TokenStream {
    let name = &method.sig.ident;
    let output = &method.sig.output;
    let types: Vec<_> = method
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(typed) => Some(&*typed.ty),
            FnArg::Receiver(_) => None,
        })
        .collect();
    let names: Vec<_> = (0..types.len())
        .map(|index| format_ident!("__a{index}"))
        .collect();

    quote! {
        #[doc(hidden)]
        #[allow(clippy::too_many_arguments)]
        async fn #wrapper(this: ::std::sync::Arc<Self> #(, #names: #types)*) #output {
            this.#name(#(#names),*).await
        }
    }
}

fn routes_fn(krate: &Path, groups: &[Group]) -> TokenStream {
    let routes = groups.iter().map(|group| {
        let pattern: &LitStr = &group.pattern;
        let endpoints = group
            .endpoints
            .iter()
            .map(|endpoint| endpoint_expr(krate, endpoint));

        quote!(.route(#krate::router::Route::new(#pattern)? #(.endpoint(#endpoints))*))
    });

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

fn endpoint_expr(krate: &Path, endpoint: &Endpoint) -> TokenStream {
    let constructor = format_ident!("{}", HttpMethod::name(endpoint.method));
    let handler = &endpoint.handler;
    let guard = guard(krate, &endpoint.when);

    quote!(#krate::router::#constructor(#handler) #guard)
}

fn guard(krate: &Path, when: &[Condition]) -> TokenStream {
    if when.is_empty() {
        return TokenStream::new();
    }

    let conditions: Vec<_> = when
        .iter()
        .map(|condition| condition.expression(krate))
        .collect();

    quote!(.guard(|parts| true #(&& #conditions)*))
}

#[cfg(test)]
mod tests;
