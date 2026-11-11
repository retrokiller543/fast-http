use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

use super::model::{Context, Handler};
use crate::route::HttpMethod;

/// The wrapper name for a handler method.
pub fn wrapper_name(handler: &Handler) -> Ident {
    format_ident!("__fast_http_{}", handler.name)
}

/// A hidden `async fn(Arc<Self>, ..)` forwarding to a `&self` method.
///
/// It is a plain function item with the method's argument types, which is what lets the router's
/// `bind` accept borrowed extractors such as `&Parts` or `Path<&str>`.
pub fn wrapper_fn(handler: &Handler) -> TokenStream {
    let wrapper = wrapper_name(handler);
    let name = &handler.name;
    let output = &handler.output;
    let types: Vec<_> = handler.params.iter().map(|param| &param.ty).collect();
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

/// The endpoint expression for one unguarded handler.
pub fn endpoint(ctx: &Context, handler: &Handler, method: HttpMethod) -> TokenStream {
    let krate = ctx.krate;
    let constructor = format_ident!("{}", method.name());
    let target = if handler.receiver {
        let wrapper = wrapper_name(handler);

        quote!(#krate::router::bind(::std::sync::Arc::clone(&self), Self::#wrapper))
    } else {
        let name = &handler.name;

        quote!(Self::#name)
    };

    quote!(#krate::router::#constructor(#target))
}
