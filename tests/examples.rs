#![allow(dead_code)]

use bytes::Bytes;
use fast_http::core::Routes;
use http::{Method, Request, StatusCode};
use http_body_util::{BodyExt, Full};

#[path = "../examples/routes.rs"]
mod routes_example;

#[cfg(feature = "macros")]
#[path = "../examples/controller.rs"]
mod controller_example;

#[cfg(feature = "macros")]
#[path = "../examples/standalone.rs"]
mod standalone_example;

async fn send<R: Routes>(
    router: &R,
    method: Method,
    uri: &str,
    headers: &[(&str, &str)],
    body: &'static str,
) -> (StatusCode, String) {
    let mut builder = Request::builder().method(method).uri(uri);

    for (name, value) in headers {
        builder = builder.header(*name, *value);
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

const JSON: (&str, &str) = ("content-type", "application/json");

#[tokio::test]
async fn routes_header_guard_selects_the_version() {
    let router = routes_example::app().expect("routes");

    let v1 = send(&router, Method::GET, "/items/7", &[], "").await;
    let v2 = send(
        &router,
        Method::GET,
        "/items/7",
        &[("x-api-version", "2")],
        "",
    )
    .await;

    assert_eq!(v1.1, r#"{"id":7,"version":"v1"}"#);
    assert_eq!(v2.1, r#"{"id":7,"version":"v2"}"#);
}

#[tokio::test]
async fn routes_body_guard_selects_book_by_kind() {
    let router = routes_example::app().expect("routes");
    let body = r#"{"kind":"book","title":"Dune"}"#;

    let (status, text) = send(&router, Method::POST, "/items", &[JSON], body).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(text, "book Dune");
}

#[tokio::test]
async fn routes_body_guard_selects_movie_by_kind() {
    let router = routes_example::app().expect("routes");
    let body = r#"{"kind":"movie","title":"Alien","minutes":117}"#;

    let (status, text) = send(&router, Method::POST, "/items", &[JSON], body).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(text, "movie Alien (117 min)");
}

#[tokio::test]
async fn routes_unknown_kind_reaches_the_unguarded_endpoint() {
    let router = routes_example::app().expect("routes");

    let (status, text) = send(
        &router,
        Method::POST,
        "/items",
        &[JSON],
        r#"{"kind":"game"}"#,
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(text, "unknown kind");
}

#[tokio::test]
async fn routes_body_that_is_not_json_reaches_the_unguarded_endpoint() {
    let router = routes_example::app().expect("routes");

    let (status, _) = send(&router, Method::POST, "/items", &[JSON], "not json").await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[cfg(feature = "macros")]
#[tokio::test]
async fn controller_staff_header_guard_lists_everything() {
    let router = controller_example::app().expect("routes");

    let staff = send(&router, Method::GET, "/items", &[("x-staff", "1")], "").await;
    let public = send(&router, Method::GET, "/items", &[], "").await;

    assert!(staff.1.contains(r#""scope":"all""#));
    assert!(public.1.contains(r#""scope":"first page""#));
}

#[cfg(feature = "macros")]
#[tokio::test]
async fn controller_query_guard_selects_the_sort_order() {
    let router = controller_example::app().expect("routes");

    let named = send(&router, Method::GET, "/items/search?sort=name", &[], "").await;
    let default = send(&router, Method::GET, "/items/search", &[], "").await;

    assert!(named.1.contains(r#""scope":"by name""#));
    assert!(default.1.contains(r#""scope":"by id""#));
}

#[cfg(feature = "macros")]
#[tokio::test]
async fn controller_header_guard_selects_the_version() {
    let router = controller_example::app().expect("routes");

    let v1 = send(&router, Method::GET, "/items/7", &[], "").await;
    let v2 = send(
        &router,
        Method::GET,
        "/items/7",
        &[("x-api-version", "2")],
        "",
    )
    .await;

    assert_eq!(v1.1, r#"{"id":7,"version":"v1"}"#);
    assert_eq!(v2.1, r#"{"id":7,"version":"v2"}"#);
}

#[cfg(feature = "macros")]
#[tokio::test]
async fn controller_body_guard_reads_the_typed_body() {
    let router = controller_example::app().expect("routes");

    let express = send(
        &router,
        Method::POST,
        "/orders",
        &[JSON],
        r#"{"item":7,"express":true}"#,
    )
    .await;
    let standard = send(
        &router,
        Method::POST,
        "/orders",
        &[JSON],
        r#"{"item":7,"express":false}"#,
    )
    .await;

    assert_eq!(express.1, "express order for item 7");
    assert_eq!(standard.1, "standard order for item 7");
}

#[cfg(feature = "macros")]
#[tokio::test]
async fn controller_body_that_does_not_parse_is_a_client_error() {
    let router = controller_example::app().expect("routes");

    let (status, _) = send(&router, Method::POST, "/orders", &[JSON], "not json").await;

    assert!(status.is_client_error());
}

#[cfg(feature = "macros")]
#[tokio::test]
async fn controller_typed_path_variants_handle_zero_separately() {
    let router = controller_example::app().expect("routes");

    let zero = send(&router, Method::GET, "/items/0", &[], "").await;
    let seven = send(&router, Method::GET, "/items/7", &[], "").await;

    assert_eq!(zero.1, r#"{"id":0,"version":"bad"}"#);
    assert_eq!(seven.1, r#"{"id":7,"version":"v1"}"#);
}

#[cfg(feature = "macros")]
#[tokio::test]
async fn controller_body_guard_without_a_receiver_creates_only_the_allowed_item() {
    let router = controller_example::app().expect("routes");

    let created = send(&router, Method::POST, "/items", &[JSON], r#"{"id":20}"#).await;
    let refused = send(&router, Method::POST, "/items", &[JSON], r#"{"id":5}"#).await;

    assert_eq!(created.0, StatusCode::CREATED);
    assert_eq!(created.1, r#"{"id":20,"version":"created"}"#);
    assert_eq!(refused.0, StatusCode::BAD_REQUEST);
}

#[cfg(feature = "macros")]
#[tokio::test]
async fn standalone_lone_functions_are_complete_endpoints() {
    let router = standalone_example::app().expect("routes");

    let health = send(&router, Method::GET, "/health", &[], "").await;
    let admin = send(&router, Method::GET, "/admin", &[("x-admin", "1")], "").await;
    let refused = send(&router, Method::GET, "/admin", &[], "").await;

    assert_eq!(health.1, "ok");
    assert_eq!(admin.1, "welcome, admin");
    assert_eq!(refused.0, StatusCode::BAD_REQUEST);
}

#[cfg(feature = "macros")]
#[tokio::test]
async fn standalone_endpoints_on_one_route_fall_through() {
    let router = standalone_example::app().expect("routes");

    let named = send(&router, Method::GET, "/greet?name=ada", &[], "").await;
    let anonymous = send(&router, Method::GET, "/greet", &[], "").await;

    assert_eq!(named.1, "hello, ada");
    assert_eq!(anonymous.1, "hello, world");
}

#[cfg(feature = "macros")]
#[tokio::test]
async fn standalone_function_returns_data_borrowed_from_its_path() {
    let router = standalone_example::app().expect("routes");

    let echoed = send(&router, Method::GET, "/echo/ada", &[], "").await;

    assert_eq!(echoed.1, "ada");
}

#[cfg(feature = "macros")]
#[tokio::test]
async fn standalone_module_variants_and_ordering() {
    let router = standalone_example::app().expect("routes");

    let all = send(&router, Method::GET, "/items", &[("x-all", "1")], "").await;
    let first = send(&router, Method::GET, "/items", &[], "").await;
    let zero = send(&router, Method::GET, "/items/0", &[], "").await;
    let seven = send(&router, Method::GET, "/items/7", &[], "").await;
    let express = send(
        &router,
        Method::POST,
        "/items/orders",
        &[JSON],
        r#"{"item":7,"express":true}"#,
    )
    .await;
    let standard = send(
        &router,
        Method::POST,
        "/items/orders",
        &[JSON],
        r#"{"item":7,"express":false}"#,
    )
    .await;

    assert_eq!(all.1, "all items");
    assert_eq!(first.1, "first page");
    assert_eq!(zero.1, "no such item");
    assert_eq!(seven.1, "item 7");
    assert_eq!(express.1, "express order for item 7");
    assert_eq!(standard.1, "standard order for item 7");
}
