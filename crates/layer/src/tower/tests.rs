use std::task::{Context, Poll};

use fast_http_core::{BoxError, HandlerError, Response};
use http::{HeaderValue, StatusCode};
use http_body_util::Full;
use tower::util::{MapRequestLayer, MapResponseLayer};
use tower_service::Service;

use super::*;
use crate::testing::{Marking, Tag, header, router, send, text};

/// Layer whose service fails without calling the inner one.
#[derive(Clone)]
struct FailLayer;

struct Fail<S>(S);

impl<S> Layer<S> for FailLayer {
    type Service = Fail<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Fail(inner)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("overloaded")]
struct Overloaded;

impl<S, R> Service<R> for Fail<S> {
    type Response = Response;
    type Error = BoxError;
    type Future = std::future::Ready<Result<Response, BoxError>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: R) -> Self::Future {
        std::future::ready(Err(Box::new(Overloaded)))
    }
}

#[tokio::test]
async fn tower_map_request_layer_edits_request() {
    let router = router().layer(TowerFlow::new(MapRequestLayer::new(
        |mut req: Request<TowerBody>| {
            req.headers_mut()
                .insert("x-trace", HeaderValue::from_static("tower"));

            req
        },
    )));

    let response = send(&router, "/trace").await;

    assert_eq!(text(response).await, "tower");
}

#[tokio::test]
async fn tower_layer_with_foreign_body_is_streamed() {
    let router = router().layer(TowerFlow::new(MapResponseLayer::new(
        |response: Response| response.map(|_| Full::new(Bytes::from_static(b"replaced"))),
    )));

    let response = send(&router, "/trace").await;

    assert!(matches!(response.body(), ResponseBody::Stream(_)));
    assert_eq!(text(response).await, "replaced");
}

#[tokio::test]
async fn tower_layer_keeping_body_type_does_not_box() {
    let router = router().layer(TowerFlow::new(MapResponseLayer::new(
        |response: Response| response,
    )));

    let response = send(&router, "/trace").await;

    assert!(matches!(response.body(), ResponseBody::Bytes(_)));
}

#[tokio::test]
async fn tower_layer_error_goes_through_error_handler() {
    let router = router()
        .with_error_handler(Marking)
        .layer(TowerFlow::new(FailLayer));

    let response = send(&router, "/trace").await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(header(&response, "x-handled"), "yes");
}

#[tokio::test]
async fn tower_layer_error_uses_custom_mapper() {
    let router = router().layer(
        TowerFlow::new(FailLayer)
            .map_err(|_| HandlerError::new(fast_http_core::Rejection::NotFound)),
    );

    let response = send(&router, "/trace").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn tower_flow_mixes_with_native_flows_and_is_spawnable() {
    let router = router()
        .layer(Tag("native"))
        .layer(TowerFlow::new(MapResponseLayer::new(
            |response: Response| response,
        )));

    let response = tokio::spawn(async move { send(&router, "/trace").await })
        .await
        .expect("task completes");

    assert_eq!(text(response).await, "native");
}
