use quote::quote;
use syn::parse_quote;

use super::*;

fn parse(attr: Attribute) -> syn::Result<RouteAttr> {
    RouteAttr::from_attribute(&attr).expect("a route attribute")
}

#[test]
fn from_attribute_other_attribute_is_none() {
    let attr: Attribute = parse_quote!(#[doc = "text"]);

    assert!(RouteAttr::from_attribute(&attr).is_none());
}

#[test]
fn parse_method_and_path() {
    let route = parse(parse_quote!(#[post("/chat")])).expect("parses");

    assert!(route.method == HttpMethod::Post);
    assert_eq!(route.path.value(), "/chat");
    assert!(route.when.is_empty());
}

#[test]
fn parse_bare_attribute_has_an_empty_path() {
    let route = parse(parse_quote!(#[get])).expect("parses");

    assert_eq!(route.path.value(), "");
}

#[test]
fn parse_name_value_attribute_is_an_error() {
    let error = parse(parse_quote!(#[get = "/a"]))
        .map(drop)
        .expect_err("error");

    assert!(error.to_string().contains("expected `#[method("));
}

#[test]
fn parse_when_collects_every_condition() {
    let route = parse(parse_quote!(#[get("/a", when(query.x = 1, header.y))])).expect("parses");

    assert_eq!(route.when.len(), 2);
    assert!(!route.when_text.is_empty());
}

#[test]
fn parse_when_without_a_path_has_an_empty_path() {
    let route = parse(parse_quote!(#[post(when(query.stream = true))])).expect("parses");

    assert_eq!(route.path.value(), "");
    assert_eq!(route.when.len(), 1);
}

#[test]
fn parse_trailing_comma_after_the_path_is_accepted() {
    let route = parse(parse_quote!(#[get("/a",)])).expect("parses");

    assert_eq!(route.path.value(), "/a");
}

#[test]
fn parse_unknown_argument_without_a_path_is_an_error() {
    let error = parse(parse_quote!(#[get(other(1))]))
        .map(drop)
        .expect_err("error");

    assert!(error.to_string().contains("expected `when(..)`"));
}

#[test]
fn parse_unknown_trailing_argument_is_an_error() {
    let error = parse(parse_quote!(#[get("/a", other(1))]))
        .map(drop)
        .expect_err("error");

    assert!(error.to_string().contains("expected `when(..)`"));
}

#[test]
fn name_matches_the_router_constructor() {
    assert_eq!(HttpMethod::Delete.name(), quote!(delete).to_string());
}
