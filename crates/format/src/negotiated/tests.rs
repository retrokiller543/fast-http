use bytes::Bytes;
use fast_http_core::MediaType;
use http::{Request, header::ACCEPT};
use serde::Serialize;

use super::*;
use crate::{Cons, Encode, Format};

#[derive(Serialize)]
struct User {
    name: &'static str,
}

struct Shout;

impl Format for Shout {
    const MEDIA_TYPE: MediaType = MediaType::new("text/x-shout");
}

impl Encode<User> for Shout {
    fn encode(value: &User, _parts: &Parts) -> Result<Bytes, FormatError> {
        Ok(Bytes::from(value.name.to_uppercase()))
    }
}

fn parts(accept: Option<&str>) -> Parts {
    let mut builder = Request::builder();

    if let Some(accept) = accept {
        builder = builder.header(ACCEPT, accept);
    }

    builder
        .body(())
        .expect("request should build")
        .into_parts()
        .0
}

fn content_type(response: &Response) -> &str {
    response.headers()[CONTENT_TYPE]
        .to_str()
        .expect("content type should be text")
}

#[cfg(feature = "json")]
#[test]
fn into_outcome_default_set_without_accept_uses_json() {
    let response = Negotiated::<_>::new(User { name: "ada" })
        .into_outcome(&parts(None))
        .expect("negotiation should succeed");

    assert_eq!(content_type(&response), "application/json");
    assert_eq!(response.headers()[VARY], "accept");
}

#[cfg(feature = "json")]
#[test]
fn into_outcome_custom_format_added_to_default_set_is_selectable() {
    let response = Negotiated::<_, Cons<Shout, Enabled>>::new(User { name: "ada" })
        .status(StatusCode::CREATED)
        .into_outcome(&parts(Some("text/x-shout")))
        .expect("negotiation should succeed");

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(content_type(&response), "text/x-shout");
}

#[test]
fn into_outcome_no_acceptable_format_is_not_acceptable() {
    let error = Negotiated::<_, (Shout,)>::new(User { name: "ada" })
        .into_outcome(&parts(Some("image/png")))
        .expect_err("image/png is not offered");

    assert_eq!(error.status(), StatusCode::NOT_ACCEPTABLE);
}
