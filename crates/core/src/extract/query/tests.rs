use http::Request;

use super::*;

fn view(uri: &str) -> (Parts, ()) {
    (
        Request::builder()
            .uri(uri)
            .body(())
            .expect("valid request")
            .into_parts()
            .0,
        (),
    )
}

#[test]
fn get_returns_the_first_value() {
    let (parts, ()) = view("/c?a=1&b=2&a=3");

    assert_eq!(QueryParams::of(&parts).get("a"), Some("1"));
}

#[test]
fn get_all_returns_every_value_in_order() {
    let (parts, ()) = view("/c?a=1&b=2&a=3");
    let values: Vec<_> = QueryParams::of(&parts).get_all("a").collect();

    assert_eq!(values, ["1", "3"]);
}

#[test]
fn get_value_less_parameter_is_empty() {
    let (parts, ()) = view("/c?debug");

    assert_eq!(QueryParams::of(&parts).get("debug"), Some(""));
}

#[test]
fn get_absent_parameter_is_none() {
    let (parts, ()) = view("/c?a=1");

    assert_eq!(QueryParams::of(&parts).get("b"), None);
}

#[test]
fn get_does_not_match_a_longer_name() {
    let (parts, ()) = view("/c?streaming=true");

    assert!(!QueryParams::of(&parts).contains("stream"));
}

#[test]
fn of_uri_without_a_query_is_empty() {
    let (parts, ()) = view("/c");

    assert!(QueryParams::of(&parts).is_empty());
    assert_eq!(QueryParams::of(&parts).raw(), "");
}

#[test]
fn from_params_borrows_from_the_request() {
    let (parts, ()) = view("/c?stream=true");
    let params = PathParams::new(&[], &[]);
    let query = <QueryParams as FromParams>::from_params(&parts, &params).expect("infallible");

    assert_eq!(query.get("stream"), Some("true"));
}
