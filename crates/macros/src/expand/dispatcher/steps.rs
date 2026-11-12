use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, Lifetime, Pat};

use super::{
    super::model::{Context, Handler, Variant},
    arg_var, slot_var,
};
use crate::guard::{Analyzed, Reserved, analyze};

/// The block that tries one variant: its guards in order, then the call.
pub fn variant_block(
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
        let make = extraction(rt, handler, *used);
        let fill = quote!(#rt::fill(&mut #slot, #make).await?);

        match plain_binding(&handler.params[*used].pat) {
            Some(name) => quote! {
                let #name = {
                    use #rt::{ViewInner as _, ViewSelf as _};

                    (&#rt::Guarded(#fill)).view()
                };
            },
            None => {
                let pat = &handler.params[*used].pat;

                quote!(let #pat = #fill;)
            }
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

/// The name a parameter is bound to when it is a plain name, such as `job` in `job: Body<Job>`.
///
/// A guard sees such a parameter through `fast_http_core::Transparent` when its
/// extractor is one, so `job` reads as `&Job`. A destructuring pattern, such as `Query(params)`,
/// is bound as written instead.
fn plain_binding(pat: &Pat) -> Option<&Ident> {
    match pat {
        Pat::Ident(ident) if ident.subpat.is_none() => Some(&ident.ident),
        _ => None,
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
