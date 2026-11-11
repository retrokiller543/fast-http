use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{Attribute, Error, Ident, Type};

use self::{slots::SlotTable, steps::variant_block};
use super::{
    model::{Context, Handler},
    partition::Partition,
};
use crate::guard::analyze;

mod slots;
mod steps;

/// How the generated endpoint struct is named and exposed.
pub struct Naming {
    /// The struct's name, or `None` to derive one from the controller and method.
    pub name: Option<Ident>,
    /// The struct's visibility; private when empty.
    pub vis: TokenStream,
    /// Tells apart derived names of endpoints that share an owner, handler and method.
    pub index: usize,
}

impl Naming {
    /// A private struct with a derived name, made unique by `index`.
    pub fn derived(index: usize) -> Self {
        Self {
            name: None,
            vis: TokenStream::new(),
            index,
        }
    }
}

/// The generated endpoint for a method and path with guarded variants: its items and the
/// expression that builds it inside `routes`.
pub struct Dispatcher {
    /// The struct and its `Endpoint` impl, emitted next to the controller's impl block.
    pub items: TokenStream,
    /// The expression that constructs the endpoint inside `routes`.
    pub expr: TokenStream,
    /// The `#[cfg(..)]` attributes of its handlers, which gate it and its registration.
    pub cfgs: Vec<Attribute>,
}

/// Generates one endpoint that picks among `partition`'s variants.
///
/// Work happens as late as it can and only once. Within a variant, arguments are extracted in the
/// order its guards need them, so a guard on the head runs before any body is parsed. An
/// argument type that several variants take is extracted once and shared. The request body is
/// never buffered: every variant that reads it must take the same body type, which is parsed
/// lazily, once.
pub fn dispatcher(
    ctx: &Context,
    handlers: &[Handler],
    partition: &Partition,
    naming: Naming,
) -> syn::Result<Dispatcher> {
    let krate = ctx.krate;
    let rt = quote!(#krate::router::dispatch);
    let alias = quote!(__rt);
    let table = SlotTable::build(handlers, &partition.variants);
    let derived = naming.name.is_none();
    let name = naming
        .name
        .unwrap_or_else(|| struct_name(ctx, handlers, partition, naming.index));
    let vis = &naming.vis;
    let mut blocks = Vec::new();

    for (index, variant) in partition.variants.iter().enumerate() {
        blocks.push(variant_block(
            ctx,
            &alias,
            handlers,
            variant,
            table.slots(index),
            index,
        )?);
    }

    let last_guarded = partition
        .variants
        .last()
        .is_some_and(|variant| !variant.guards.is_empty());
    let miss = last_guarded.then(|| quote!(::std::result::Result::Ok(::std::option::Option::None)));
    let slot_vars: Vec<_> = (0..table.types.len()).map(slot_var).collect();
    let slot_types = &table.types;
    let rule = table.body_rule(&rt);
    let generics = ctx.owner.as_ref().map(|owner| owner.generics);
    let checked_early = generics.is_none_or(|generics| generics.params.is_empty());
    let early_rule = rule
        .as_ref()
        .filter(|_| checked_early)
        .map(|rule| quote!(const _: () = #rule;));
    let inline_rule = rule
        .as_ref()
        .filter(|_| !checked_early)
        .map(|rule| quote!(const { #rule };));
    let method = format_ident!("{}", partition.method.constant());
    let core = quote!(#krate::core);
    let (impl_generics, type_generics, where_clause) = match generics {
        Some(generics) => {
            let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

            (
                quote!(#impl_generics),
                quote!(#type_generics),
                quote!(#where_clause),
            )
        }
        None => (TokenStream::new(), TokenStream::new(), TokenStream::new()),
    };
    let (fields, this_binding, construct) = match &ctx.owner {
        Some(owner) => {
            let self_ty = owner.self_ty;

            (
                quote!({ this: ::std::sync::Arc<#self_ty>, }),
                quote!(let __this = &self.this;),
                quote!(#name { this: ::std::sync::Arc::clone(&self) }),
            )
        }
        None => (quote!(;), TokenStream::new(), quote!(#name)),
    };
    let hidden = derived.then(|| quote!(#[doc(hidden)]));
    let cfgs = shared_cfgs(handlers, partition)?;
    let guards_read_body = guards_read_body(&rt, handlers, partition)?;
    let items = quote! {
        #(#cfgs)*
        #early_rule

        #(#cfgs)*
        #hidden
        #[allow(non_camel_case_types)]
        #vis struct #name #impl_generics #where_clause #fields

        #(#cfgs)*
        impl #impl_generics #core::Endpoint for #name #type_generics #where_clause {
            fn method(&self) -> &#core::http::Method {
                &#core::http::Method::#method
            }

            async fn call<'__r, __B: #core::ReqBody, __H: #core::ErrorHandler>(
                &self,
                __parts: &'__r #core::http::request::Parts,
                __params: #core::PathParams<'__r>,
                __body: __B,
                __handler: &__H,
            ) -> #core::Response {
                match self.attempt(__parts, __params, __body, __handler).await {
                    #core::Attempt::Done(response) => response,
                    #core::Attempt::Declined(_) | #core::Attempt::Missed => {
                        #rt::fail(#rt::guard_miss(), __parts, __handler).await
                    }
                }
            }

            #guards_read_body

            #[allow(unused_variables, unused_mut)]
            async fn attempt<'__r, __B: #core::ReqBody, __H: #core::ErrorHandler>(
                &self,
                __parts: &'__r #core::http::request::Parts,
                __params: #core::PathParams<'__r>,
                __body: __B,
                __handler: &__H,
            ) -> #core::Attempt<__B> {
                use #rt as __rt;
                #inline_rule
                #this_binding
                let mut __source = __rt::BodySource::new(__body);
                #( let mut #slot_vars = __rt::slot::<#slot_types>(); )*

                let __result: ::std::result::Result<
                    ::std::option::Option<#core::Response>,
                    #core::HandlerError,
                > = async {
                    #(#blocks)*
                    #miss
                }
                .await;

                match __result {
                    ::std::result::Result::Ok(::std::option::Option::Some(response)) => {
                        #core::Attempt::Done(response)
                    }
                    ::std::result::Result::Ok(::std::option::Option::None) => {
                        match __source.into_unread() {
                            ::std::option::Option::Some(body) => #core::Attempt::Declined(body),
                            ::std::option::Option::None => #core::Attempt::Missed,
                        }
                    }
                    ::std::result::Result::Err(error) => {
                        #core::Attempt::Done(__rt::fail(error, __parts, __handler).await)
                    }
                }
            }
        }
    };

    Ok(Dispatcher {
        items,
        expr: construct,
        cfgs,
    })
}

/// The `#[cfg(..)]` attributes of the handlers in `partition`, which must all be the same, since
/// one endpoint calls every one of them.
fn shared_cfgs(handlers: &[Handler], partition: &Partition) -> syn::Result<Vec<Attribute>> {
    let mut variants = partition
        .variants
        .iter()
        .map(|variant| &handlers[variant.handler]);
    let Some(first) = variants.next() else {
        return Ok(Vec::new());
    };
    let key = |handler: &Handler| {
        handler
            .cfgs
            .iter()
            .map(|cfg| cfg.to_token_stream().to_string())
            .collect::<Vec<_>>()
    };
    let expected = key(first);

    for handler in variants {
        if key(handler) != expected {
            return Err(Error::new_spanned(
                &handler.name,
                format!(
                    "`{}` and `{}` answer the same method and path, so they need the same #[cfg(..)] attributes",
                    first.name, handler.name
                ),
            ));
        }
    }

    Ok(first.cfgs.clone())
}

/// `Endpoint::guards_read_body`, when a guard of some variant uses that variant's last argument,
/// which is the one that may read the body.
fn guards_read_body(
    rt: &TokenStream,
    handlers: &[Handler],
    partition: &Partition,
) -> syn::Result<Option<TokenStream>> {
    let mut types = Vec::new();

    for variant in &partition.variants {
        let handler = &handlers[variant.handler];
        let Some(last) = handler.params.len().checked_sub(1) else {
            continue;
        };

        for guard in &variant.guards {
            if analyze(guard, &handler.params)?.uses.contains(&last) {
                types.push(&handler.params[last].ty);

                break;
            }
        }
    }

    if types.is_empty() {
        return Ok(None);
    }

    Ok(Some(quote! {
        fn guards_read_body(&self) -> bool {
            false #(|| #rt::uses_body::<#types, _>())*
        }
    }))
}

fn struct_name(ctx: &Context, handlers: &[Handler], partition: &Partition, index: usize) -> Ident {
    let owner = match ctx.owner.as_ref().map(|owner| owner.self_ty) {
        Some(Type::Path(path)) => path.path.segments.last().map_or_else(
            || String::from("Controller"),
            |segment| segment.ident.to_string(),
        ),
        Some(_) => String::from("Controller"),
        None => String::from("Free"),
    };
    let first = partition
        .variants
        .first()
        .map_or_else(String::new, |variant| {
            handlers[variant.handler].name.to_string()
        });

    format_ident!(
        "__FastHttp_{}_{}_{}_{}",
        owner,
        first,
        partition.method.constant(),
        index
    )
}

fn slot_var(index: usize) -> Ident {
    format_ident!("__s{index}")
}

fn arg_var(index: usize) -> Ident {
    format_ident!("__a{index}")
}
