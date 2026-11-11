use std::collections::{BTreeSet, HashMap};

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::Type;

use super::super::model::{Handler, Variant};

/// The argument slots of a generated endpoint: one per distinct argument type, shared by the
/// variants that take it.
pub struct SlotTable<'a> {
    /// The type each slot holds.
    pub types: Vec<&'a Type>,
    per_variant: Vec<Vec<usize>>,
    last_slots: BTreeSet<usize>,
}

impl<'a> SlotTable<'a> {
    /// Assigns a slot to every parameter of every variant.
    pub fn build(handlers: &'a [Handler], variants: &[&Variant]) -> Self {
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

    /// The slot of each parameter of variant `variant`.
    pub fn slots(&self, variant: usize) -> &[usize] {
        &self.per_variant[variant]
    }

    /// The assertion that at most one body type is read, when more than one type could be.
    pub fn body_rule(&self, rt: &TokenStream) -> Option<TokenStream> {
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
