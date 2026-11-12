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

fn extract<'r, T: serde::Deserialize<'r> + Send>(
    parts: &'r Parts,
) -> Result<Query<T>, FormatError> {
    Query::from_params_at(parts, &PathParams::EMPTY)
}

#[test]
fn from_parts_reads_typed_fields() {
    let Query(params) = extract::<Params>(&parts("/c?stream=true&page=2")).expect("ok");

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
    let Query(params) = extract::<Params>(&parts("/c")).expect("ok");

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
    let error = extract::<Params>(&parts("/c?page=many"))
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
fn query_borrows_until_decoding_is_needed() {
    let plain = parts("/c?name=ada");
    let encoded = parts("/c?name=a%20b");

    let Query(plain) = extract::<Borrowing>(&plain).expect("decodes");
    let Query(encoded) = extract::<Borrowing>(&encoded).expect("decodes");

    assert!(matches!(plain.name, std::borrow::Cow::Borrowed("ada")));
    assert!(matches!(encoded.name, std::borrow::Cow::Owned(_)));
    assert_eq!(encoded.name, "a b");
}

#[test]
fn query_rejects_a_plain_str_field_that_needs_decoding() {
    #[derive(Debug, Deserialize)]
    struct Strict<'a> {
        #[allow(dead_code)]
        name: &'a str,
    }

    let request = parts("/c?name=a%20b");

    let error = extract::<Strict>(&request).expect_err("cannot borrow decoded text");

    let FormatError::Decode(source) = &error else {
        panic!("a decode error, got {error:?}");
    };
    let borrowed = source
        .downcast_ref::<BorrowedQueryField>()
        .expect("the borrowing hint is a typed error");

    assert!(error.to_string().contains("Cow"));
    assert!(std::error::Error::source(borrowed).is_some());
}
