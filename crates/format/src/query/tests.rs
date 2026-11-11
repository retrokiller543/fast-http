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
