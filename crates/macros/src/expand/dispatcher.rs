use std::collections::{BTreeSet, HashMap};

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::{Ident, Lifetime, Pat, Type};

use super::{
    model::{Context, Handler, Param, Variant},
    partition::Partition,
};
use crate::guard::{Analyzed, Reserved, analyze};

/// A step of a variant's plan: extract one argument, or evaluate one guard.
enum Step {
    Extract(usize),
    Guard(usize),
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
/// buffered only when different variants need different body types.
pub fn dispatcher(
    ctx: &Context,
    handlers: &[Handler],
    partition: &Partition,
) -> syn::Result<Dispatcher> {
    let krate = ctx.krate;
    let rt = quote!(#krate::router::dispatch);
    let table = SlotTable::build(handlers, &partition.variants);
    let name = struct_name(ctx, handlers, partition);
    let mut blocks = Vec::new();

    for (index, variant) in partition.variants.iter().enumerate() {
        blocks.push(variant_block(
            ctx,
            &rt,
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
    let miss = last_guarded.then(|| quote!(#rt::guard_miss(__parts, __handler).await));
    let slot_vars: Vec<_> = (0..table.types.len()).map(slot_var).collect();
    let multi = table.multi_expression(&rt);
    let method = format_ident!("{}", partition.method.constant());
    let core = quote!(#krate::core);
    let (impl_generics, type_generics, where_clause) = ctx.generics.split_for_impl();
    let self_ty = ctx.self_ty;
    let items = quote! {
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        struct #name #impl_generics #where_clause {
            this: ::std::sync::Arc<#self_ty>,
        }

        impl #impl_generics #core::Endpoint for #name #type_generics #where_clause {
            fn method(&self) -> &#core::http::Method {
                &#core::http::Method::#method
            }

            #[allow(unused_variables, unused_mut)]
            async fn call<'r, B: #core::ReqBody, H: #core::ErrorHandler>(
                &self,
                __parts: &'r #core::http::request::Parts,
                __params: #core::PathParams<'r>,
                __body: B,
                __handler: &H,
            ) -> #core::Response {
                let __this = &self.this;
                let __multi = #multi;
                let mut __source = #rt::BodySource::new(__body, __multi);
                #( let mut #slot_vars = ::std::option::Option::None; )*

                #(#blocks)*
                #miss
            }
        }
    };

    Ok(Dispatcher {
        items,
        expr: quote!(#name { this: ::std::sync::Arc::clone(&self) }),
    })
}

fn struct_name(ctx: &Context, handlers: &[Handler], partition: &Partition) -> Ident {
    let owner = match ctx.self_ty {
        Type::Path(path) => path.path.segments.last().map_or_else(
            || String::from("Controller"),
            |segment| segment.ident.to_string(),
        ),
        _ => String::from("Controller"),
    };
    let first = partition
        .variants
        .first()
        .map_or_else(String::new, |variant| {
            handlers[variant.handler].name.to_string()
        });

    format_ident!(
        "__FastHttp_{}_{}_{}",
        owner,
        first,
        partition.method.constant()
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

    fn multi_expression(&self, rt: &TokenStream) -> TokenStream {
        if self.last_slots.len() < 2 {
            return quote!(false);
        }

        let terms = self.last_slots.iter().map(|slot| {
            let ty = self.types[*slot];

            quote!(::std::primitive::usize::from(#rt::uses_body::<#ty, _>()))
        });

        quote!(0_usize #(+ #terms)* > 1)
    }
}

fn plan(params: usize, guards: &[Analyzed]) -> Vec<Step> {
    let mut extracted = vec![false; params];
    let mut steps = Vec::new();

    for (index, guard) in guards.iter().enumerate() {
        for used in &guard.uses {
            if !extracted[*used] {
                extracted[*used] = true;
                steps.push(Step::Extract(*used));
            }
        }

        steps.push(Step::Guard(index));
    }

    for (index, done) in extracted.iter().enumerate() {
        if !done {
            steps.push(Step::Extract(index));
        }
    }

    steps
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
    let steps: Vec<_> = plan(handler.params.len(), &guards)
        .into_iter()
        .map(|step| match step {
            Step::Extract(param) => extract_step(ctx, rt, handler, slots, param),
            Step::Guard(guard) => guard_step(
                ctx,
                rt,
                &handler.params,
                slots,
                &guards[guard],
                label.as_ref(),
            ),
        })
        .collect();
    let call = call_step(ctx, rt, handler, slots);

    Ok(match label {
        Some(label) => quote!(#label: { #(#steps)* #call }),
        None => quote!({ #(#steps)* #call }),
    })
}

fn extract_step(
    ctx: &Context,
    rt: &TokenStream,
    handler: &Handler,
    slots: &[usize],
    param: usize,
) -> TokenStream {
    let _ = ctx;
    let slot = slot_var(slots[param]);
    let ty = &handler.params[param].ty;
    let make = if param + 1 == handler.params.len() {
        quote!(#rt::extract_last::<#ty, _, _>(__parts, &__params, &mut __source))
    } else {
        quote!(async { #rt::extract_head::<#ty>(__parts, &__params) })
    };

    quote! {
        if let ::std::result::Result::Err(error) = #rt::fill(&mut #slot, #make).await {
            return #rt::fail(error, __parts, __handler).await;
        }
    }
}

fn guard_step(
    ctx: &Context,
    rt: &TokenStream,
    params: &[Param],
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
        let pat = binding_pattern(&params[*used].pat);

        quote! {
            let #pat = match &#slot {
                ::std::option::Option::Some(value) => value,
                ::std::option::Option::None => return #rt::unfilled(__parts, __handler).await,
            };
        }
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
    let takes: Vec<_> = slots.iter().map(|slot| slot_var(*slot)).collect();
    let name = &handler.name;
    let call = if handler.receiver {
        quote!(__this.#name(#(#args),*))
    } else {
        let self_ty = ctx.self_ty;

        quote!(<#self_ty>::#name(#(#args),*))
    };

    quote! {
        let (#(::std::option::Option::Some(#args),)*) = (#(#takes.take(),)*) else {
            return #rt::unfilled(__parts, __handler).await;
        };

        return #rt::finish(#call.await, __parts, __handler).await;
    }
}
