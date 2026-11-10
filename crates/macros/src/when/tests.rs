use quote::quote;

use super::*;

fn parse(tokens: TokenStream) -> syn::Result<Condition> {
    syn::parse2(tokens)
}

fn expression(tokens: TokenStream) -> String {
    let krate: Path = syn::parse_quote!(::fast_http);

    parse(tokens)
        .expect("parses")
        .expression(&krate)
        .to_string()
        .replace(' ', "")
}

fn error(tokens: TokenStream) -> String {
    parse(tokens).map(drop).expect_err("error").to_string()
}

#[test]
fn expression_query_equals_boolean() {
    assert_eq!(
        expression(quote!(query.stream = true)),
        "::fast_http::core::guard::query_eq(parts,\"stream\",\"true\")"
    );
}

#[test]
fn expression_query_equals_integer() {
    assert!(expression(quote!(query.page = 2)).contains("\"page\",\"2\""));
}

#[test]
fn expression_query_equals_string() {
    assert!(expression(quote!(query.mode = "fast")).contains("\"mode\",\"fast\""));
}

#[test]
fn expression_query_presence() {
    assert_eq!(
        expression(quote!(query.debug)),
        "::fast_http::core::guard::query_has(parts,\"debug\")"
    );
}

#[test]
fn expression_query_name_may_be_a_keyword() {
    assert!(expression(quote!(query.type = "a")).contains("\"type\""));
}

#[test]
fn expression_header_identifier_becomes_a_header_name() {
    assert!(expression(quote!(header.X_Api_Key)).contains("\"x-api-key\""));
}

#[test]
fn expression_header_string_name_is_lowercased() {
    assert!(expression(quote!(header."Content-Type" = "a/b")).contains("\"content-type\",\"a/b\""));
}

#[test]
fn expression_not_negates() {
    assert!(expression(quote!(not(query.debug))).starts_with("!("));
}

#[test]
fn expression_any_joins_with_or() {
    assert!(expression(quote!(any(query.a, query.b))).contains("||"));
}

#[test]
fn expression_all_joins_with_and() {
    assert!(expression(quote!(all(query.a, query.b))).contains("&&"));
}

#[test]
fn expression_guard_calls_the_head_guard() {
    assert_eq!(
        expression(quote!(guard(my::Admin))),
        "<my::Adminas::fast_http::core::HeadGuard>::check(parts).is_some()"
    );
}

#[test]
fn parse_empty_any_is_an_error() {
    assert!(error(quote!(any())).contains("at least one"));
}

#[test]
fn parse_not_with_two_conditions_is_an_error() {
    assert!(error(quote!(not(query.a, query.b))).contains("exactly one"));
}

#[test]
fn parse_body_is_a_pointed_error() {
    assert!(error(quote!(body.json.stream = true)).contains("body guards are not supported"));
}

#[test]
fn parse_unknown_keyword_is_an_error() {
    assert!(error(quote!(cookie.session)).contains("expected `query`"));
}

#[test]
fn parse_float_value_is_an_error() {
    assert!(error(quote!(query.x = 1.5)).contains("string, integer or boolean"));
}
