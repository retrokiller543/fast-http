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
fn constant_matches_http() {
    assert_eq!(HttpMethod::Delete.constant(), "DELETE");
}

#[test]
fn parse_valid_guards_report_no_errors() {
    let route = parse(parse_quote!(#[get(when(a.x, b.y))])).expect("parses");

    assert!(route.errors.is_empty());
    assert_eq!(route.guards.len(), 2);
}

#[test]
fn parse_a_broken_guard_is_recorded_and_the_others_survive() {
    let route = parse(parse_quote!(#[get(when(a.x, b., c.z))])).expect("parses");

    assert_eq!(route.guards.len(), 3);
    assert_eq!(route.errors.len(), 1);
}

#[test]
fn parse_a_trailing_comma_in_when_is_accepted() {
    let route = parse(parse_quote!(#[get(when(a.x,))])).expect("parses");

    assert_eq!(route.guards.len(), 1);
    assert!(route.errors.is_empty());
}

#[test]
fn route_general_form_takes_the_method_first() {
    let route = parse(parse_quote!(#[route(POST, "/chat", when(a.x))])).expect("parses");

    assert!(route.method == HttpMethod::Post);
    assert_eq!(route.path.value(), "/chat");
    assert_eq!(route.guards.len(), 1);
}

#[test]
fn route_general_form_accepts_any_case_and_an_omitted_path() {
    let route = parse(parse_quote!(#[route(get, when(a.x))])).expect("parses");

    assert!(route.method == HttpMethod::Get);
    assert_eq!(route.path.value(), "");
}

#[test]
fn route_general_form_with_only_a_method_is_a_bare_route() {
    let route = parse(parse_quote!(#[route(Delete)])).expect("parses");

    assert!(route.method == HttpMethod::Delete);
}

#[test]
fn route_unknown_method_is_an_error() {
    assert!(failure(parse_quote!(#[route(FETCH, "/a")])).contains("expected GET, POST"));
}

#[test]
fn route_without_arguments_is_an_error() {
    assert!(failure(parse_quote!(#[route])).contains("expected `#[route(METHOD"));
}

#[test]
fn is_route_recognizes_the_general_form() {
    assert!(RouteAttr::is_route(&parse_quote!(#[route(GET, "/a")])));
    assert!(RouteAttr::is_route(&parse_quote!(#[get("/a")])));
    assert!(!RouteAttr::is_route(&parse_quote!(#[doc = "x"])));
}

#[test]
fn parse_crate_override_in_any_position() {
    let before = parse(parse_quote!(#[get("/a", crate = ::web, when(a.x))])).expect("parses");
    let after = parse(parse_quote!(#[get(when(a.x), crate = ::web)])).expect("parses");

    for route in [before, after] {
        let krate = route.krate.expect("crate is set");

        assert_eq!(quote::quote!(#krate).to_string(), ":: web");
        assert_eq!(route.guards.len(), 1);
    }
}

#[test]
fn parse_crate_given_twice_is_an_error() {
    assert!(failure(parse_quote!(#[get(crate = ::a, crate = ::b)])).contains("given twice"));
}
