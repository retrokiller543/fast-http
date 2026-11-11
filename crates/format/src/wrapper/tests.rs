use std::collections::HashSet;

use crate::{Body, Negotiated};

/// A format set that implements none of the std traits.
struct Opaque;

#[test]
fn value_traits_need_only_the_carried_value_to_have_them() {
    let body: Body<u32, Opaque> = 7.into();
    let copy = body;
    let text: Body<String, Opaque> = String::from("ada").into();
    let mut seen = HashSet::new();

    seen.insert(text.clone());

    assert_eq!(copy, body);
    assert!(seen.contains(&text));
    assert_eq!(format!("{body:?}"), "Body(7)");
    assert!(body <= Body::from(8));
    assert_eq!(Body::<u32, Opaque>::default().into_inner(), 0);
}

#[test]
fn access_traits_reach_the_value() {
    let mut body: Body<String, Opaque> = String::from("ada").into();

    body.push_str(" lovelace");

    assert_eq!(body.len(), 12);
    assert_eq!(AsRef::<String>::as_ref(&body), "ada lovelace");
    assert_eq!(body.into_inner(), "ada lovelace");
}

#[test]
fn negotiated_reaches_its_value_and_keeps_its_status_when_cloned() {
    let negotiated: Negotiated<u32, Opaque> = Negotiated::new(3).status(http::StatusCode::CREATED);

    let cloned = negotiated.clone();

    assert_eq!(*cloned, 3);
    assert!(format!("{cloned:?}").contains("201"));
}
