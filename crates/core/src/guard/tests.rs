use http::{Request, request::Parts};

use super::*;

fn parts(uri: &str, header: Option<(&str, &str)>) -> Parts {
    let mut builder = Request::builder().uri(uri);

    if let Some((name, value)) = header {
        builder = builder.header(name, value);
    }

    builder.body(()).expect("valid request").into_parts().0
}

#[test]
fn query_eq_matching_value_is_true() {
    assert!(query_eq(
        &parts("/chat?stream=true", None),
        "stream",
        "true"
    ));
}

#[test]
fn query_eq_other_value_is_false() {
    assert!(!query_eq(
        &parts("/chat?stream=false", None),
        "stream",
        "true"
    ));
}

#[test]
fn query_eq_absent_parameter_is_false() {
    assert!(!query_eq(&parts("/chat", None), "stream", "false"));
}

#[test]
fn query_eq_finds_the_parameter_among_others() {
    assert!(query_eq(
        &parts("/c?a=1&stream=true&b=2", None),
        "stream",
        "true"
    ));
}

#[test]
fn query_eq_matches_any_repeated_occurrence() {
    assert!(query_eq(&parts("/c?x=1&x=2", None), "x", "2"));
}

#[test]
fn query_eq_does_not_match_a_longer_name() {
    assert!(!query_eq(
        &parts("/c?streaming=true", None),
        "stream",
        "true"
    ));
}

#[test]
fn query_has_value_less_parameter_is_true() {
    assert!(query_has(&parts("/c?debug", None), "debug"));
}

#[test]
fn query_has_empty_value_parameter_is_true() {
    assert!(query_has(&parts("/c?debug=", None), "debug"));
}

#[test]
fn query_has_absent_parameter_is_false() {
    assert!(!query_has(&parts("/c?other=1", None), "debug"));
}

#[test]
fn query_has_without_a_query_is_false() {
    assert!(!query_has(&parts("/c", None), "debug"));
}

#[test]
fn header_eq_matching_value_is_true() {
    assert!(header_eq(
        &parts("/", Some(("x-mode", "fast"))),
        "x-mode",
        "fast"
    ));
}

#[test]
fn header_eq_other_value_is_false() {
    assert!(!header_eq(
        &parts("/", Some(("x-mode", "slow"))),
        "x-mode",
        "fast"
    ));
}

#[test]
fn header_has_present_header_is_true() {
    assert!(header_has(&parts("/", Some(("x-mode", "slow"))), "x-mode"));
}

#[test]
fn header_has_absent_header_is_false() {
    assert!(!header_has(&parts("/", None), "x-mode"));
}
