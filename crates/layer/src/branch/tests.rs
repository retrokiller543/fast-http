use fast_http_core::{HeadGuard, Identity};
use http::request::Parts;

use super::*;
use crate::testing::{Tag, header, router, send, text};

struct Admin;

impl HeadGuard for Admin {
    type Output = ();

    fn check(parts: &Parts) -> Option<()> {
        parts.headers.contains_key("x-admin").then_some(())
    }
}

#[tokio::test]
async fn handle_predicate_true_takes_then_arm() {
    let router = router().layer(Branch::new(
        |parts| parts.uri.query().is_some(),
        Tag("a"),
        Tag("b"),
    ));

    let response = send(&router, "/trace?x").await;

    assert_eq!(text(response).await, "a");
}

#[tokio::test]
async fn handle_predicate_false_takes_otherwise_arm() {
    let router = router().layer(Branch::new(
        |parts| parts.uri.query().is_some(),
        Tag("a"),
        Tag("b"),
    ));

    let response = send(&router, "/trace").await;

    assert_eq!(header(&response, "x-trace-out"), "b");
}

#[tokio::test]
async fn on_head_guard_miss_takes_otherwise_arm() {
    let router = router().layer(Branch::on::<Admin>(Tag("admin"), Identity));

    let response = send(&router, "/trace").await;

    assert_eq!(text(response).await, "-");
}
