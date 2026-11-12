use std::sync::atomic::{AtomicUsize, Ordering};

use fast_http_core::{ErrorHandler, Flow, Next, ReqBody, Request, Response};
use http::StatusCode;

use super::*;
use crate::testing::{Deny, Tag, router, send, text};

static MIRRORED: AtomicUsize = AtomicUsize::new(0);

struct Count(&'static AtomicUsize);

impl Flow for Count {
    async fn handle<B: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        req: Request<B>,
        next: N,
        _: &H,
    ) -> Response {
        if req.headers().contains_key("x-mirror") {
            self.0.fetch_add(1, Ordering::SeqCst);
        }

        next.run(req).await
    }
}

#[tokio::test]
async fn handle_runs_primary_and_returns_its_response() {
    let router = router().layer(Tee::new(Tag("p"), Tag("s")));

    let response = send(&router, "/trace").await;

    assert_eq!(text(response).await, "p");
}

#[tokio::test]
async fn handle_shadow_sees_a_copy_of_the_head() {
    let router = router().layer(Tee::new(Tag("p"), Count(&MIRRORED)));
    let request = http::Request::builder()
        .uri("/trace")
        .header("x-mirror", "1")
        .body(http_body_util::Full::new(bytes::Bytes::new()))
        .expect("valid request");

    let response = fast_http_core::Routes::dispatch(&router, request).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(MIRRORED.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn handle_shadow_rejection_does_not_affect_primary() {
    let router = router().layer(Tee::new(Tag("p"), Deny));

    let response = send(&router, "/trace").await;

    assert_eq!(response.status(), StatusCode::OK);
}
