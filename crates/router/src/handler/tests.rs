use bytes::Bytes;
use fast_http_core::Routes;
use http::{Method, Request, StatusCode, request::Parts};
use http_body_util::{BodyExt, Full};

use crate::{Route, Router, post};

async fn book() -> &'static str {
    "book"
}

async fn other() -> &'static str {
    "other"
}

fn is_book(_: &Parts, body: &Bytes) -> bool {
    body.starts_with(b"book")
}

fn is_json(parts: &Parts, _: &Bytes) -> bool {
    parts.headers.contains_key("x-json")
}

async fn send<R: Routes>(router: &R, header: bool, body: &'static str) -> (StatusCode, String) {
    let mut builder = Request::builder().method(Method::POST).uri("/items");

    if header {
        builder = builder.header("x-json", "1");
    }

    let request = builder
        .body(Full::new(Bytes::from_static(body.as_bytes())))
        .expect("valid request");
    let response = router.dispatch(request).await;
    let status = response.status();
    let text = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    (status, String::from_utf8(text.to_vec()).expect("utf-8"))
}

fn router() -> Router<impl crate::RouteList> {
    Router::new().route(
        Route::new("/items")
            .expect("pattern")
            .endpoint(post(book).body_guard(is_book))
            .endpoint(post(other)),
    )
}

#[tokio::test]
async fn body_guard_matching_body_selects_the_endpoint() {
    assert_eq!(send(&router(), false, "book: x").await.1, "book");
}

#[tokio::test]
async fn body_guard_other_body_falls_through_to_the_next_endpoint() {
    assert_eq!(send(&router(), false, "movie: x").await.1, "other");
}

#[tokio::test]
async fn body_guard_without_a_fallback_is_a_guard_miss() {
    let router = Router::new().route(
        Route::new("/items")
            .expect("pattern")
            .endpoint(post(book).body_guard(is_book)),
    );

    assert_eq!(
        send(&router, false, "movie").await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn body_guard_sees_the_head_too() {
    let router = Router::new().route(
        Route::new("/items")
            .expect("pattern")
            .endpoint(post(book).body_guard(is_json)),
    );

    assert_eq!(send(&router, true, "x").await.1, "book");
    assert_eq!(send(&router, false, "x").await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn head_guard_rejection_skips_the_body_guard() {
    let router = Router::new().route(
        Route::new("/items")
            .expect("pattern")
            .endpoint(
                post(book)
                    .guard(|parts| parts.headers.contains_key("x-json"))
                    .body_guard(|_, _| panic!("body guard must not run")),
            )
            .endpoint(post(other)),
    );

    assert_eq!(send(&router, false, "book").await.1, "other");
}

#[tokio::test]
async fn unguarded_get_is_unaffected() {
    let router = Router::new().route(Route::new("/items").expect("pattern").get(book));
    let request = Request::builder()
        .uri("/items")
        .body(Full::new(Bytes::new()))
        .expect("valid request");

    assert_eq!(router.dispatch(request).await.status(), StatusCode::OK);
}
