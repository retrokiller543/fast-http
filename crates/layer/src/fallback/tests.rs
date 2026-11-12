use fast_http_core::{Identity, Routes};
use fast_http_router::{Route, Router};
use http::StatusCode;

use super::*;
use crate::testing::{Deny, Tag, header, router, send, text};

/// A request extension whose value tells which flow set it last.
#[derive(Clone)]
struct Mark(&'static str);

/// Sets [`Mark`] on the request and continues.
struct SetMark(&'static str);

impl Flow for SetMark {
    async fn handle<B: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        mut req: Request<B>,
        next: N,
        _: &H,
    ) -> Response {
        req.extensions_mut().insert(Mark(self.0));

        next.run(req).await
    }
}

async fn mark(parts: &Parts) -> String {
    parts
        .extensions
        .get::<Mark>()
        .map_or("-", |mark| mark.0)
        .to_owned()
}

fn marked(flow: impl Flow) -> impl Routes {
    Router::new()
        .route(Route::new("/mark").expect("valid pattern").get(mark))
        .layer(flow)
        .layer(SetMark("original"))
}

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

#[tokio::test]
async fn handle_primary_continues_keeps_the_extensions_it_replaced() {
    let router = marked(Fallback::new(SetMark("primary"), Identity));

    let response = send(&router, "/mark").await;

    assert_eq!(text(response).await, "primary");
}

#[tokio::test]
async fn handle_primary_continues_keeps_extensions_it_did_not_touch() {
    let router = marked(Fallback::new(Identity, Identity));

    let response = send(&router, "/mark").await;

    assert_eq!(text(response).await, "original");
}
