#![cfg(feature = "macros")]

use bytes::Bytes;
use fast_http::{
    core::Routes,
    get, post, route,
    router::{Route, Router},
    routes,
};
use http::{Method, Request, StatusCode};
use http_body_util::{BodyExt, Full};

#[get("/health")]
async fn health() -> &'static str {
    "ok"
}

#[get("/deep", when(query.contains("full")))]
async fn deep_full() -> &'static str {
    "full"
}

#[get("/deep")]
async fn deep_plain() -> &'static str {
    "plain"
}

#[post("/echo", when(headers.contains_key("x-upper")))]
async fn echo_upper() -> &'static str {
    "upper"
}

#[route(GET, "/general", when(query.contains("on")))]
async fn general() -> &'static str {
    "general"
}

#[route(post, "/general-post")]
async fn general_post() -> &'static str {
    "posted"
}

#[routes("/items")]
mod items {
    use std::num::NonZeroU64;

    use fast_http::{core::Path, format::Body};
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct Order {
        item: u64,
        express: bool,
    }

    #[get("/{id}", when(*id == 0))]
    async fn zero(Path(id): Path<u64>) -> &'static str {
        "zero"
    }

    #[get("/{id}")]
    async fn show(Path(id): Path<NonZeroU64>) -> String {
        format!("item:{id}")
    }

    #[route(GET, "/mixed", when(query.contains("m")))]
    async fn mixed_guarded() -> &'static str {
        "mixed guarded"
    }

    #[get("/mixed")]
    async fn mixed() -> &'static str {
        "mixed"
    }

    #[get(when(headers.contains_key("x-all")))]
    async fn all() -> &'static str {
        "all"
    }

    #[get]
    async fn first() -> &'static str {
        "first"
    }

    #[post("/orders", when(order.express))]
    async fn express(Body { value: order, .. }: Body<Order>) -> String {
        format!("express:{}", order.item)
    }

    #[post("/orders")]
    async fn standard(order: Body<Order>) -> String {
        format!("standard:{}", order.value.item)
    }
}

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

fn standalone() -> Router<impl fast_http::router::RouteList> {
    Router::new()
        .route(health.route().expect("valid route"))
        .route(
            Route::new("/deep")
                .expect("valid route")
                .endpoint(deep_full)
                .endpoint(deep_plain),
        )
}

#[tokio::test]
async fn standalone_function_is_an_endpoint_at_its_path() {
    let (status, body) = send(&standalone(), Method::GET, "/health", &[], "").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");
}

#[tokio::test]
async fn standalone_guard_reads_the_query_view() {
    let (status, _) = send(&health_with_guard(), Method::GET, "/deep?full", &[], "").await;

    assert_eq!(status, StatusCode::OK);
}

fn health_with_guard() -> Router<impl fast_http::router::RouteList> {
    Router::new().route(deep_full.route().expect("valid route"))
}

#[tokio::test]
async fn standalone_guard_miss_is_a_client_error() {
    let (status, _) = send(&health_with_guard(), Method::GET, "/deep", &[], "").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn standalone_guarded_function_reads_headers_without_declaring_them() {
    let router = Router::new().route(echo_upper.route().expect("valid route"));

    let hit = send(&router, Method::POST, "/echo", &[("x-upper", "1")], "").await;
    let miss = send(&router, Method::POST, "/echo", &[], "").await;

    assert_eq!(hit.1, "upper");
    assert_eq!(miss.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn routes_module_groups_free_functions_into_variants() {
    let router = items::routes().expect("valid routes");

    assert_eq!(
        send(&router, Method::GET, "/items/0", &[], "").await.1,
        "zero"
    );
    assert_eq!(
        send(&router, Method::GET, "/items/7", &[], "").await.1,
        "item:7"
    );
}

#[tokio::test]
async fn routes_module_variants_may_omit_the_path() {
    let router = items::routes().expect("valid routes");

    assert_eq!(
        send(&router, Method::GET, "/items", &[("x-all", "1")], "")
            .await
            .1,
        "all"
    );
    assert_eq!(
        send(&router, Method::GET, "/items", &[], "").await.1,
        "first"
    );
}

#[tokio::test]
async fn routes_module_body_guard_reads_the_typed_body() {
    let router = items::routes().expect("valid routes");
    let json = [("content-type", "application/json")];

    let express = send(
        &router,
        Method::POST,
        "/items/orders",
        &json,
        r#"{"item":3,"express":true}"#,
    )
    .await;
    let standard = send(
        &router,
        Method::POST,
        "/items/orders",
        &json,
        r#"{"item":3,"express":false}"#,
    )
    .await;

    assert_eq!(express.1, "express:3");
    assert_eq!(standard.1, "standard:3");
}

#[tokio::test]
async fn routes_module_merges_into_a_router() {
    let router = Router::new().merge(items::routes().expect("valid routes"));

    assert_eq!(
        send(&router, Method::GET, "/items/9", &[], "").await.1,
        "item:9"
    );
}

#[tokio::test]
async fn route_general_form_works_on_lone_functions() {
    let router = Router::new()
        .route(general.route().expect("valid route"))
        .route(general_post.route().expect("valid route"));

    let hit = send(&router, Method::GET, "/general?on", &[], "").await;
    let post = send(&router, Method::POST, "/general-post", &[], "").await;

    assert_eq!(hit.1, "general");
    assert_eq!(post.1, "posted");
}

#[tokio::test]
async fn route_general_form_mixes_with_the_shorthands_in_a_module() {
    let router = items::routes().expect("valid routes");

    assert_eq!(
        send(&router, Method::GET, "/items/mixed?m", &[], "")
            .await
            .1,
        "mixed guarded"
    );
    assert_eq!(
        send(&router, Method::GET, "/items/mixed", &[], "").await.1,
        "mixed"
    );
}
