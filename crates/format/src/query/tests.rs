use http::{Request, StatusCode};
use serde::Deserialize;

use super::*;

#[derive(Debug, Deserialize, PartialEq)]
struct Params {
    stream: Option<bool>,
    page: Option<u32>,
}

fn parts(uri: &str) -> Parts {
    Request::builder()
        .uri(uri)
        .body(())
        .expect("valid request")
        .into_parts()
        .0
}

#[test]
fn from_parts_reads_typed_fields() {
    let Query(params) = Query::<Params>::from_parts(&parts("/c?stream=true&page=2")).expect("ok");

    assert_eq!(
        params,
        Params {
            stream: Some(true),
            page: Some(2)
        }
    );
}

#[test]
fn from_parts_missing_optional_fields_are_none() {
    let Query(params) = Query::<Params>::from_parts(&parts("/c")).expect("ok");

    assert_eq!(
        params,
        Params {
            stream: None,
            page: None
        }
    );
}

#[test]
fn from_parts_wrong_type_is_a_bad_request() {
    let error = Query::<Params>::from_parts(&parts("/c?page=many"))
        .map(drop)
        .expect_err("rejected");

    assert_eq!(
        fast_http_core::ErrorResponse::status(&error),
        StatusCode::BAD_REQUEST
    );
}

#[derive(Debug, Deserialize)]
struct Borrowing<'a> {
    #[serde(borrow)]
    name: std::borrow::Cow<'a, str>,
}

#[test]
fn raw_query_borrows_until_decoding_is_needed() {
    let plain = parts("/c?name=ada");
    let encoded = parts("/c?name=a%20b");
    let params = PathParams::EMPTY;

    let plain = RawQuery::from_params(&plain, &params).expect("infallible");
    let encoded = RawQuery::from_params(&encoded, &params).expect("infallible");
    let plain: Borrowing = plain.deserialize().expect("decodes");
    let encoded: Borrowing = encoded.deserialize().expect("decodes");

    assert!(matches!(plain.name, std::borrow::Cow::Borrowed("ada")));
    assert!(matches!(encoded.name, std::borrow::Cow::Owned(_)));
    assert_eq!(encoded.name, "a b");
}

#[test]
fn raw_query_also_reads_types_that_own_their_fields() {
    let request = parts("/c?stream=true&page=2");
    let raw = RawQuery::from_params(&request, &PathParams::EMPTY).expect("infallible");

    let params: Params = raw.deserialize().expect("decodes");

    assert_eq!(params.page, Some(2));
}

#[test]
fn raw_query_rejects_a_plain_str_field_that_needs_decoding() {
    #[derive(Debug, Deserialize)]
    struct Strict<'a> {
        #[allow(dead_code)]
        name: &'a str,
    }

    let request = parts("/c?name=a%20b");
    let raw = RawQuery::from_params(&request, &PathParams::EMPTY).expect("infallible");

    let error = raw
        .deserialize::<Strict>()
        .expect_err("cannot borrow decoded text");

    assert!(error.to_string().contains("Cow"));
}
