use bytes::Bytes;
use http::Request;
use http_body_util::Full;
use serde::Deserialize;

use super::*;
use crate::{FormatError, formats::Json};

#[derive(Debug, PartialEq, Deserialize)]
struct User {
    name: String,
}

fn parts(content_type: Option<&str>) -> Parts {
    let mut builder = Request::builder();

    if let Some(content_type) = content_type {
        builder = builder.header(CONTENT_TYPE, content_type);
    }

    builder
        .body(())
        .expect("request should build")
        .into_parts()
        .0
}

fn body(text: &'static str) -> Full<Bytes> {
    Full::new(Bytes::from_static(text.as_bytes()))
}

#[tokio::test]
async fn from_request_matching_content_type_decodes() {
    let parsed = Body::<User>::from_request(
        &parts(Some("application/json; charset=utf-8")),
        body(r#"{"name":"ada"}"#),
    )
    .await
    .expect("json body should decode");

    assert_eq!(parsed.value.name, "ada");
}

#[tokio::test]
async fn from_request_unlisted_content_type_is_unsupported() {
    let error = Body::<User, (Json,)>::from_request(&parts(Some("text/plain")), body("x"))
        .await
        .err()
        .expect("text/plain is not accepted");

    assert!(matches!(error, FormatError::UnsupportedMediaType));
}

#[tokio::test]
async fn from_request_missing_content_type_is_rejected() {
    let error = Body::<User>::from_request(&parts(None), body("{}"))
        .await
        .err()
        .expect("content type is required");

    assert!(matches!(error, FormatError::MissingContentType));
}

#[tokio::test]
async fn from_request_over_limit_is_too_large() {
    let error = Body::<User, Enabled, 4>::from_request(
        &parts(Some("application/json")),
        body(r#"{"name":"ada"}"#),
    )
    .await
    .err()
    .expect("body exceeds the 4 byte limit");

    assert!(matches!(error, FormatError::TooLarge { limit: 4 }));
}
