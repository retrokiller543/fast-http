use proc_macro2::Span;
use syn::LitStr;

use super::*;
use crate::route::HttpMethod;

fn variant(pattern: &str, method: HttpMethod) -> Variant {
    Variant {
        handler: 0,
        method,
        pattern: LitStr::new(pattern, Span::call_site()),
        guards: Vec::new(),
        when_text: String::new(),
    }
}

fn order(patterns: &[&str]) -> Vec<String> {
    let variants: Vec<_> = patterns
        .iter()
        .map(|pattern| variant(pattern, HttpMethod::Get))
        .collect();

    plan(&variants)
        .expect("plans")
        .iter()
        .map(|plan| plan.pattern.value())
        .collect()
}

#[test]
fn plan_orders_literals_before_parameters() {
    assert_eq!(
        order(&["/items/{id}", "/items/orders"]),
        ["/items/orders", "/items/{id}"]
    );
}

#[test]
fn plan_orders_parameters_before_wildcards_before_rest() {
    assert_eq!(
        order(&["/f/{**path}", "/f/*", "/f/{id}"]),
        ["/f/{id}", "/f/*", "/f/{**path}"]
    );
}

#[test]
fn plan_keeps_declaration_order_among_equally_specific_routes() {
    assert_eq!(
        order(&["/b/{x}", "/a/{y}", "/c/{z}"]),
        ["/b/{x}", "/a/{y}", "/c/{z}"]
    );
}

#[test]
fn plan_compares_segment_by_segment() {
    assert_eq!(
        order(&["/{a}/orders", "/items/{b}"]),
        ["/items/{b}", "/{a}/orders"]
    );
}

#[test]
fn plan_keeps_the_variants_of_a_route_in_declaration_order() {
    let mut first = variant("/a", HttpMethod::Get);
    let mut second = variant("/a", HttpMethod::Get);

    first.when_text = String::from("one");
    second.when_text = String::from("two");
    first.guards = vec![syn::parse_quote!(true)];

    let variants = [first, second];
    let plans = plan(&variants).expect("plans");
    let texts: Vec<_> = plans[0].partitions[0]
        .variants
        .iter()
        .map(|variant| variant.when_text.as_str())
        .collect();

    assert_eq!(texts, ["one", "two"]);
}
