use fast_http_core::Identity;
use http::StatusCode;

use super::*;
use crate::testing::{Deny, Tag, header, router, send, text};

#[tokio::test]
async fn handle_primary_continues_uses_primary() {
    let router = router().layer(Fallback::new(Tag("a"), Tag("b")));

    let response = send(&router, "/trace").await;

    assert_eq!(text(response).await, "a");
}

#[tokio::test]
async fn handle_primary_rejects_runs_secondary() {
    let router = router().layer(Fallback::new(Deny, Tag("b")));

    let response = send(&router, "/trace").await;

    assert_eq!(text(response).await, "b");
}

#[tokio::test]
async fn handle_both_reject_returns_secondary_rejection() {
    let router = router().layer(Fallback::new(Deny, Deny));

    let response = send(&router, "/trace").await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn handle_response_not_matching_when_skips_secondary() {
    let router =
        router().layer(Fallback::new(Deny, Tag("b")).when(|response| response.status() == 404));

    let response = send(&router, "/trace").await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn handle_primary_continues_keeps_its_header_changes() {
    let router = router().layer(Fallback::new(Tag("a"), Identity));

    let response = send(&router, "/trace").await;

    assert_eq!(header(&response, "x-trace-out"), "a");
}
