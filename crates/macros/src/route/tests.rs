use syn::parse_quote;

use super::*;

fn parse(attr: Attribute) -> syn::Result<RouteAttr> {
    RouteAttr::from_attribute(&attr).expect("a route attribute")
}

fn failure(attr: Attribute) -> String {
    parse(attr).map(drop).expect_err("error").to_string()
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
    assert!(route.guards.is_empty());
}

#[test]
fn parse_bare_attribute_has_an_empty_path() {
    let route = parse(parse_quote!(#[get])).expect("parses");

    assert_eq!(route.path.value(), "");
}

#[test]
fn parse_when_without_a_path_has_an_empty_path() {
    let route = parse(parse_quote!(#[post(when(q.stream))])).expect("parses");

    assert_eq!(route.path.value(), "");
    assert_eq!(route.guards.len(), 1);
}

#[test]
fn parse_when_keeps_each_expression() {
    let route = parse(parse_quote!(#[get("/a", when(q.x == 1, headers.contains_key("y")))]))
        .expect("parses");

    assert_eq!(route.guards.len(), 2);
    assert!(!route.when_text.is_empty());
}

#[test]
fn parse_when_accepts_any_rust_expression() {
    let route = parse(parse_quote!(#[get(when(matches!(q.mode, Some(Mode::Fast)) && !q.debug))]))
        .expect("parses");

    assert_eq!(route.guards.len(), 1);
}

#[test]
fn parse_trailing_comma_after_the_path_is_accepted() {
    let route = parse(parse_quote!(#[get("/a",)])).expect("parses");

    assert_eq!(route.path.value(), "/a");
}

#[test]
fn parse_empty_when_is_an_error() {
    assert!(failure(parse_quote!(#[get(when())])).contains("needs a condition"));
}

#[test]
fn parse_unknown_trailing_argument_is_an_error() {
    assert!(failure(parse_quote!(#[get("/a", other(1))])).contains("expected `when(..)`"));
}

#[test]
fn parse_unknown_argument_without_a_path_is_an_error() {
    assert!(failure(parse_quote!(#[get(other(1))])).contains("expected `when(..)`"));
}

#[test]
fn parse_name_value_attribute_is_an_error() {
    assert!(failure(parse_quote!(#[get = "/a"])).contains("expected `#[method("));
}

#[test]
fn name_and_constant_match_the_router_and_http() {
    assert_eq!(HttpMethod::Delete.name(), "delete");
    assert_eq!(HttpMethod::Delete.constant(), "DELETE");
}
