use std::sync::Arc;

use bytes::Bytes;
use fast_http_core::{Path, Routes};
use http::{Request, StatusCode, request::Parts};
use http_body_util::{BodyExt, Full};

use crate::{Route, Router};

use super::bind;

struct Db(&'static str);

async fn count(db: Arc<Db>) -> String {
    db.0.to_owned()
}

async fn show(db: Arc<Db>, Path(id): Path<u64>) -> String {
    format!("{}:{id}", db.0)
}

async fn describe(db: Arc<Db>, Path(id): Path<u64>, parts: &Parts) -> String {
    format!("{}:{id}:{}", db.0, parts.uri.path())
}

async fn dispatch<R: Routes>(router: &R, uri: &str) -> (StatusCode, String) {
    let request = Request::builder()
        .uri(uri)
        .body(Full::new(Bytes::new()))
        .expect("valid request");
    let response = router.dispatch(request).await;
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    (status, String::from_utf8(body.to_vec()).expect("utf-8"))
}

#[tokio::test]
async fn bind_state_only_handler_receives_the_state() {
    let router = Router::new().route(
        Route::new("/count")
            .expect("pattern")
            .get(bind(Arc::new(Db("users")), count)),
    );

    assert_eq!(dispatch(&router, "/count").await.1, "users");
}

#[tokio::test]
async fn bind_handler_receives_state_and_extractor() {
    let router = Router::new().route(
        Route::new("/users/{id}")
            .expect("pattern")
            .get(bind(Arc::new(Db("users")), show)),
    );

    assert_eq!(dispatch(&router, "/users/7").await.1, "users:7");
}

#[tokio::test]
async fn bind_handler_may_borrow_the_request_head() {
    let router = Router::new().route(
        Route::new("/users/{id}")
            .expect("pattern")
            .get(bind(Arc::new(Db("users")), describe)),
    );

    assert_eq!(dispatch(&router, "/users/7").await.1, "users:7:/users/7");
}

#[tokio::test]
async fn bind_extractor_failure_is_rendered_by_the_error_handler() {
    let router = Router::new().route(
        Route::new("/users/{id}")
            .expect("pattern")
            .get(bind(Arc::new(Db("users")), show)),
    );

    assert_eq!(
        dispatch(&router, "/users/abc").await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn bind_shares_one_state_between_routes() {
    let db = Arc::new(Db("shared"));
    let router = Router::new()
        .route(
            Route::new("/a/{id}")
                .expect("pattern")
                .get(bind(Arc::clone(&db), show)),
        )
        .route(
            Route::new("/b/{id}")
                .expect("pattern")
                .get(bind(Arc::clone(&db), show)),
        );

    assert_eq!(dispatch(&router, "/b/1").await.1, "shared:1");
    assert_eq!(Arc::strong_count(&db), 3);
}
