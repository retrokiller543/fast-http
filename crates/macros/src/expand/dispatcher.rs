use std::collections::{BTreeSet, HashMap};

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::{Ident, Lifetime, Pat, Type};

use super::{
    model::{Context, Handler, Variant},
    partition::Partition,
};
use crate::guard::{Analyzed, Reserved, analyze};

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
    let items = quote! {
        #early_rule

        #hidden
        #[allow(non_camel_case_types)]
        #vis struct #name #impl_generics #where_clause #fields

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
                    #core::Attempt::Declined(_) => {
                        #rt::fail(#rt::guard_miss(), __parts, __handler).await
                    }
                }
            }

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
                            ::std::option::Option::None => #core::Attempt::Done(
                                __rt::fail(__rt::guard_miss(), __parts, __handler).await,
                            ),
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
    })
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

struct SlotTable<'a> {
    types: Vec<&'a Type>,
    per_variant: Vec<Vec<usize>>,
    last_slots: BTreeSet<usize>,
}

impl<'a> SlotTable<'a> {
    fn build(handlers: &'a [Handler], variants: &[&Variant]) -> Self {
        let mut keys: Vec<String> = Vec::new();
        let mut types: Vec<&Type> = Vec::new();
        let mut per_variant = Vec::new();
        let mut last_slots = BTreeSet::new();

        for variant in variants {
            let handler = &handlers[variant.handler];
            let mut seen: HashMap<String, usize> = HashMap::new();
            let mut slots = Vec::new();

            for param in &handler.params {
                let base = param.ty.to_token_stream().to_string();
                let occurrence = seen.entry(base.clone()).or_insert(0);
                let key = format!("{base}#{occurrence}");

                *occurrence += 1;

                let slot = keys
                    .iter()
                    .position(|known| *known == key)
                    .unwrap_or_else(|| {
                        keys.push(key);
                        types.push(&param.ty);

                        keys.len() - 1
                    });

                slots.push(slot);
            }

            if let Some(last) = slots.last() {
                last_slots.insert(*last);
            }

            per_variant.push(slots);
        }

        Self {
            types,
            per_variant,
            last_slots,
        }
    }

    fn slots(&self, variant: usize) -> &[usize] {
        &self.per_variant[variant]
    }

    /// The assertion that at most one body type is read, when more than one type could be.
    fn body_rule(&self, rt: &TokenStream) -> Option<TokenStream> {
        if self.last_slots.len() < 2 {
            return None;
        }

        let terms = self.last_slots.iter().map(|slot| {
            let ty = self.types[*slot];

            quote!((#rt::uses_body::<#ty, _>() as ::std::primitive::usize))
        });

        Some(quote! {
            ::std::assert!(
                0_usize #(+ #terms)* <= 1,
                "variants of one method and path that read the request body must all take the same body type; define one enum for the body and guard on its variants"
            )
        })
    }
}

fn variant_block(
    ctx: &Context,
    rt: &TokenStream,
    handlers: &[Handler],
    variant: &Variant,
    slots: &[usize],
    index: usize,
) -> syn::Result<TokenStream> {
    let handler = &handlers[variant.handler];
    let guards = variant
        .guards
        .iter()
        .map(|expr| analyze(expr, &handler.params))
        .collect::<syn::Result<Vec<_>>>()?;
    let label =
        (!guards.is_empty()).then(|| Lifetime::new(&format!("'__v{index}"), Span::call_site()));
    let steps: Vec<_> = guards
        .iter()
        .map(|guard| guard_step(ctx, rt, handler, slots, guard, label.as_ref()))
        .collect();
    let call = call_step(ctx, rt, handler, slots);

    Ok(match label {
        Some(label) => quote!(#label: { #(#steps)* #call }),
        None => quote!({ #(#steps)* #call }),
    })
}

/// The expression that extracts parameter `param`, which is only run when its slot is empty.
fn extraction(rt: &TokenStream, handler: &Handler, param: usize) -> TokenStream {
    let ty = &handler.params[param].ty;

    if param + 1 == handler.params.len() {
        quote!(#rt::extract_last::<#ty, _, _>(__parts, &__params, &mut __source))
    } else {
        quote!(async { #rt::extract_head::<#ty>(__parts, &__params) })
    }
}

fn guard_step(
    ctx: &Context,
    rt: &TokenStream,
    handler: &Handler,
    slots: &[usize],
    guard: &Analyzed,
    label: Option<&Lifetime>,
) -> TokenStream {
    let expr = guard.expr;
    let krate = ctx.krate;
    let views = guard.reserved.iter().map(|reserved| match reserved {
        Reserved::Headers => quote!(let headers = &__parts.headers;),
        Reserved::Query => quote!(let query = #krate::core::QueryParams::of(__parts);),
    });
    let lets = guard.uses.iter().map(|used| {
        let slot = slot_var(slots[*used]);
        let pat = binding_pattern(&handler.params[*used].pat);
        let make = extraction(rt, handler, *used);

        quote!(let #pat = #rt::fill(&mut #slot, #make).await?;)
    });

    quote! {
        {
            #(#views)*
            #(#lets)*

            if !(#expr) {
                break #label;
            }
        }
    }
}

fn binding_pattern(pat: &Pat) -> TokenStream {
    match pat {
        Pat::Ident(ident) if ident.subpat.is_none() => {
            let name = &ident.ident;

            quote!(#name)
        }
        other => quote!(#other),
    }
}

fn call_step(ctx: &Context, rt: &TokenStream, handler: &Handler, slots: &[usize]) -> TokenStream {
    let args: Vec<_> = (0..handler.params.len()).map(arg_var).collect();
    let takes = args.iter().enumerate().map(|(param, arg)| {
        let slot = slot_var(slots[param]);
        let make = extraction(rt, handler, param);

        quote!(let #arg = #rt::take(&mut #slot, #make).await?;)
    });
    let name = &handler.name;
    let call = match (&ctx.owner, handler.receiver) {
        (_, true) => quote!(__this.#name(#(#args),*)),
        (Some(owner), false) => {
            let self_ty = owner.self_ty;

            quote!(<#self_ty>::#name(#(#args),*))
        }
        (None, false) => quote!(#name(#(#args),*)),
    };

    quote! {
        #(#takes)*

        return ::std::result::Result::Ok(::std::option::Option::Some(
            #rt::finish(#call.await, __parts, __handler).await,
        ));
    }
}
