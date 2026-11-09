use http::HeaderValue;

use super::*;
use crate::testing::{Tag, header, router, send, text};

#[tokio::test]
async fn map_request_edits_head_before_handler() {
    let router = router().layer(map_request(|parts: &mut Parts| {
        parts
            .headers
            .insert("x-trace", HeaderValue::from_static("mapped"));
    }));

    let response = send(&router, "/trace").await;

    assert_eq!(text(response).await, "mapped");
}

#[tokio::test]
async fn map_response_sees_handler_response() {
    let router = router().layer(map_response(|mut response: Response| {
        response
            .headers_mut()
            .insert("x-mapped", HeaderValue::from_static("1"));

        response
    }));

    let response = send(&router, "/trace").await;

    assert_eq!(header(&response, "x-mapped"), "1");
}

#[tokio::test]
async fn layer_last_added_runs_outermost() {
    let router = router().layer(Tag("inner")).layer(Tag("outer"));

    let response = send(&router, "/trace").await;

    assert_eq!(header(&response, "x-trace-out"), "inner,outer");
    assert_eq!(text(response).await, "outer,inner");
}
