use proc_macro2::Span;

use super::*;

fn lit(text: &str) -> LitStr {
    LitStr::new(text, Span::call_site())
}

fn joined(prefix: Option<&str>, route: &str) -> String {
    let prefix = prefix.map(lit);

    join(prefix.as_ref(), &lit(route)).expect("joins").value()
}

#[test]
fn join_prefix_and_route() {
    assert_eq!(joined(Some("/users"), "/{id}"), "/users/{id}");
}

#[test]
fn join_empty_route_is_the_prefix() {
    assert_eq!(joined(Some("/users"), ""), "/users");
}

#[test]
fn join_lone_slash_under_a_prefix_is_the_prefix() {
    assert_eq!(joined(Some("/users"), "/"), "/users");
}

#[test]
fn join_without_prefix_keeps_the_route() {
    assert_eq!(joined(None, "/health"), "/health");
}

#[test]
fn join_nothing_is_the_root() {
    assert_eq!(joined(None, ""), "/");
}

#[test]
fn join_root_prefix_is_no_prefix() {
    assert_eq!(joined(Some("/"), "/a"), "/a");
}

#[test]
fn join_route_without_leading_slash_is_an_error() {
    let error = join(None, &lit("health")).map(drop).expect_err("error");

    assert!(error.to_string().contains("must start with `/`"));
}

#[test]
fn join_prefix_with_trailing_slash_is_an_error() {
    let prefix = lit("/users/");
    let error = join(Some(&prefix), &lit("/a"))
        .map(drop)
        .expect_err("error");

    assert!(error.to_string().contains("must not end with `/`"));
}

#[test]
fn join_prefix_without_leading_slash_is_an_error() {
    let prefix = lit("users");
    let error = join(Some(&prefix), &lit("/a"))
        .map(drop)
        .expect_err("error");

    assert!(error.to_string().contains("must start with `/`"));
}

fn rejected(route: &str) -> String {
    join(None, &lit(route))
        .map(drop)
        .expect_err("rejected")
        .to_string()
}

#[test]
fn join_applies_the_router_segment_rules() {
    assert!(rejected("/a//b").contains("empty segment"));
    assert!(rejected("/a/").contains("empty segment"));
    assert!(rejected("/{}").contains("unnamed parameter"));
    assert!(rejected("/{**}").contains("unnamed parameter"));
    assert!(rejected("/**/a").contains("not last"));
    assert!(rejected("/{**rest}/a").contains("not last"));
    assert!(rejected("/{id}/{id}").contains("repeats parameter `id`"));
}

#[test]
fn join_rejects_more_params_than_the_router_captures() {
    let route: String = (0..17).map(|index| format!("/{{p{index}}}")).collect();

    assert!(rejected(&route).contains("more than 16"));
}

#[test]
fn join_accepts_every_segment_kind() {
    assert_eq!(joined(None, "/a/{id}/*/{**rest}"), "/a/{id}/*/{**rest}");
    assert_eq!(joined(None, "/files/**"), "/files/**");
}
