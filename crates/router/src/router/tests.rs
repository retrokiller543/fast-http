use std::{
    pin::pin,
    task::{Context, Poll, Waker},
};

use bytes::Bytes;
use fast_http_core::{Path, Segments};
use http::{HeaderMap, Method, Request as HttpRequest, StatusCode, header, request::Parts};
use http_body_util::Full;

use super::*;
use crate::{Route, get, post};

async fn show(Path(id): Path<u64>) -> String {
    format!("user {id}")
}

async fn pair(Path((org, repo)): Path<(&str, &str)>) -> String {
    format!("{org}/{repo}")
}

async fn files(Path(rest): Path<Segments<'_>>) -> String {
    rest.iter().collect::<Vec<_>>().join("|")
}

async fn named(Path(map): Path<std::collections::BTreeMap<String, String>>) -> String {
    format!("{map:?}")
}

async fn named_borrowed(Path(map): Path<std::collections::BTreeMap<&str, &str>>) -> String {
    format!("{map:?}")
}

async fn mixed(Path((org, id)): Path<(&str, u64)>, parts: &Parts, headers: &HeaderMap) -> String {
    let agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-");

    format!("{} {org}#{id} {agent}", parts.method)
}

async fn create() -> StatusCode {
    StatusCode::CREATED
}

async fn health() -> &'static str {
    "ok"
}

fn router() -> Router<impl RouteList> {
    Router::new((
        Route::new("/users/{id}", (get(show),)).expect("valid pattern"),
        Route::new("/users", (post(create),)).expect("valid pattern"),
        Route::new("/repos/{org}/{repo}", (get(pair),)).expect("valid pattern"),
        Route::new("/files/{**rest}", (get(files),)).expect("valid pattern"),
        Route::new("/named/{a}/{b}", (get(named),)).expect("valid pattern"),
        Route::new("/named-borrowed/{a}/{b}", (get(named_borrowed),)).expect("valid pattern"),
        Route::new("/mixed/{org}/{id}", (get(mixed),)).expect("valid pattern"),
        Route::new(
            "/closure/{id}",
            (get(|Path(id): Path<u64>, method: &Method| {
                let method = method.to_string();

                async move { format!("{method} {id}") }
            }),),
        )
        .expect("valid pattern"),
        Route::new("/{**any}", (get(health),)).expect("valid pattern"),
    ))
}

fn send<L: RouteList>(
    router: &Router<L>,
    method: Method,
    uri: &str,
) -> (StatusCode, String, Response) {
    let mut cx = Context::from_waker(Waker::noop());
    let request = HttpRequest::builder()
        .method(method)
        .uri(uri)
        .header(header::USER_AGENT, "test")
        .body(Full::new(Bytes::new()))
        .expect("valid request");
    let Poll::Ready(response) = pin!(router.dispatch(request)).poll(&mut cx) else {
        panic!("dispatch should complete without suspending");
    };
    let status = response.status();
    let allow = response
        .headers()
        .get(header::ALLOW)
        .map(|value| value.to_str().expect("ascii").to_owned())
        .unwrap_or_default();

    (status, allow, response)
}

async fn body_text(response: Response) -> String {
    use http_body_util::BodyExt;

    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    String::from_utf8(bytes.to_vec()).expect("utf8")
}

#[tokio::test]
async fn dispatch_param_route_extracts_typed_path_value() {
    let router = router();

    let (status, _, response) = send(&router, Method::GET, "/users/42");

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body_text(response).await, "user 42");
}

#[tokio::test]
async fn dispatch_tuple_path_extracts_in_declaration_order() {
    let router = router();

    let (_, _, response) = send(&router, Method::GET, "/repos/rust/cargo");

    assert_eq!(body_text(response).await, "rust/cargo");
}

#[tokio::test]
async fn dispatch_double_star_exposes_segments() {
    let router = router();

    let (_, _, response) = send(&router, Method::GET, "/files/a/b/c");

    assert_eq!(body_text(response).await, "a|b|c");
}

#[tokio::test]
async fn dispatch_map_path_extracts_by_name() {
    let router = router();

    let (_, _, response) = send(&router, Method::GET, "/named/1/2");

    assert_eq!(body_text(response).await, r#"{"a": "1", "b": "2"}"#);
}

#[test]
fn dispatch_unparsable_param_returns_400() {
    let router = router();

    let (status, ..) = send(&router, Method::GET, "/users/abc");

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[test]
fn dispatch_wrong_method_returns_405_with_allow() {
    let router = router();

    let (status, allow, _) = send(&router, Method::GET, "/users");

    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(allow, "POST");
}

#[test]
fn dispatch_no_matching_route_returns_404() {
    let router = Router::new((Route::new("/a", (get(health),)).expect("valid pattern"),));

    let (status, ..) = send(&router, Method::GET, "/b");

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[test]
fn dispatch_post_literal_route_returns_created() {
    let router = router();

    let (status, ..) = send(&router, Method::POST, "/users");

    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn dispatch_borrowed_map_path_borrows_names_and_values() {
    let router = router();

    let (_, _, response) = send(&router, Method::GET, "/named-borrowed/x/y");

    assert_eq!(body_text(response).await, r#"{"a": "x", "b": "y"}"#);
}

#[tokio::test]
async fn dispatch_mixed_borrowed_and_owned_extractors() {
    let router = router();

    let (_, _, response) = send(&router, Method::GET, "/mixed/rust/7");

    assert_eq!(body_text(response).await, "GET rust#7 test");
}

#[tokio::test]
async fn dispatch_closure_handler_with_borrowed_argument() {
    let router = router();

    let (_, _, response) = send(&router, Method::GET, "/closure/9");

    assert_eq!(body_text(response).await, "GET 9");
}

#[tokio::test]
async fn dispatch_future_with_borrowed_extractors_is_spawnable() {
    let router = router();
    let request = HttpRequest::builder()
        .uri("/mixed/rust/7")
        .body(Full::new(Bytes::new()))
        .expect("valid request");

    let response = tokio::spawn(async move { router.dispatch(request).await })
        .await
        .expect("task completes");

    assert_eq!(response.status(), StatusCode::OK);
}
