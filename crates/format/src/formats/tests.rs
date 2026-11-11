#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct User {
    name: String,
    age: u8,
}

fn user() -> User {
    User {
        name: "ada".to_owned(),
        age: 36,
    }
}

fn roundtrip<F: Encode<User> + Decode<User>>() {
    let parts = http::Request::new(()).into_parts().0;

    let bytes = F::encode(&user(), &parts).expect("user should encode");
    let decoded = F::decode(&bytes).expect("encoded user should decode");

    assert_eq!(decoded, user());
}

#[cfg(feature = "json")]
#[test]
fn json_roundtrip_preserves_value() {
    roundtrip::<Json>();
}

#[cfg(feature = "form")]
#[test]
fn form_roundtrip_preserves_value() {
    roundtrip::<Form>();
}

#[cfg(feature = "yaml")]
#[test]
fn yaml_roundtrip_preserves_value() {
    roundtrip::<Yaml>();
}

#[cfg(feature = "toml")]
#[test]
fn toml_roundtrip_preserves_value() {
    roundtrip::<Toml>();
}

#[cfg(feature = "cbor")]
#[test]
fn cbor_roundtrip_preserves_value() {
    roundtrip::<Cbor>();
}

#[cfg(feature = "msgpack")]
#[test]
fn msgpack_roundtrip_preserves_value() {
    roundtrip::<MessagePack>();
}

#[cfg(feature = "xml")]
#[test]
fn xml_roundtrip_preserves_value() {
    roundtrip::<Xml>();
}

fn head(content_type: &str, accept: &str) -> Parts {
    http::Request::builder()
        .header(CONTENT_TYPE, content_type)
        .header(http::header::ACCEPT, accept)
        .body(())
        .expect("request should build")
        .into_parts()
        .0
}

fn body(text: &'static str) -> http_body_util::Full<Bytes> {
    http_body_util::Full::new(Bytes::from_static(text.as_bytes()))
}

#[cfg(feature = "json")]
#[tokio::test]
async fn json_wrapper_extracts_only_json() {
    let accepted = Json::<User>::from_request(
        &head("application/json", "*/*"),
        body(r#"{"name":"ada","age":36}"#),
    )
    .await
    .expect("json is accepted");
    let refused = Json::<User>::from_request(&head("application/yaml", "*/*"), body("name: ada"))
        .await
        .map(drop)
        .expect_err("yaml is not json");

    assert_eq!(accepted.name, "ada");
    assert!(matches!(refused, FormatError::UnsupportedMediaType));
}

#[cfg(feature = "json")]
#[test]
fn json_wrapper_answers_json_whatever_the_client_accepts() {
    let response = Json(user())
        .into_outcome(&head("application/json", "application/yaml"))
        .expect("encodes");

    assert_eq!(response.headers()[CONTENT_TYPE], "application/json");
}

#[cfg(feature = "yaml")]
#[tokio::test]
async fn yaml_wrapper_round_trips_on_its_own() {
    let parsed = Yaml::<User>::from_request(
        &head("application/yaml", "*/*"),
        body("name: ada\nage: 36\n"),
    )
    .await
    .expect("yaml is accepted");

    let response = parsed
        .into_outcome(&head("application/yaml", "*/*"))
        .expect("encodes");

    assert_eq!(response.headers()[CONTENT_TYPE], "application/yaml");
}

#[cfg(feature = "json")]
#[test]
fn json_wrapper_status_tuple_sets_the_status() {
    let response = (http::StatusCode::CREATED, Json(user()))
        .into_outcome(&head("application/json", "*/*"))
        .expect("encodes");

    assert_eq!(response.status(), http::StatusCode::CREATED);
}
