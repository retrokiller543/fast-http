#![cfg(feature = "macros")]

use bytes::Bytes;
use fast_http::{
    core::{Path, Routes},
    endpoints, get, post, route,
    router::Router,
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

#[get("/echo/{name}")]
#[allow(clippy::needless_lifetimes)]
async fn echo<'a>(Path(name): Path<&'a str>) -> &'a str {
    name
}

#[derive(serde::Deserialize)]
struct Ticket {
    id: u64,
    priority: bool,
}

#[post("/tickets", when(ticket.priority))]
async fn priority_ticket(ticket: fast_http::format::Body<Ticket>) -> String {
    format!("priority:{}", ticket.id)
}

#[post("/tickets")]
async fn queued_ticket(ticket: fast_http::format::Body<Ticket>) -> String {
    format!("queued:{}", ticket.id)
}

#[routes("/gated")]
mod gated {
    #[cfg(any())]
    #[get("/gone")]
    async fn gone() -> &'static str {
        "gone"
    }

    #[cfg(any())]
    #[delete("/kept")]
    async fn remove() -> &'static str {
        "removed"
    }

    #[cfg(test)]
    #[get("/kept")]
    async fn kept() -> &'static str {
        "kept"
    }
}

#[routes("/borrowed")]
mod borrowed {
    use fast_http::core::Path;

    #[get("/{name}", when(name.len() > 3))]
    #[allow(clippy::needless_lifetimes)]
    async fn long<'a>(Path(name): Path<&'a str>) -> &'a str {
        name
    }

    #[get("/{name}")]
    #[allow(clippy::needless_lifetimes)]
    async fn short<'a>(Path(name): Path<&'a str>) -> String {
        format!("short:{name}")
    }
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
    async fn express(order: Body<Order>) -> String {
        format!("express:{}", order.item)
    }

    #[post("/orders")]
    async fn standard(order: Body<Order>) -> String {
        format!("standard:{}", order.item)
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
        .route(health)
        .route(endpoints![deep_full, deep_plain])
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
    Router::new().route(deep_full)
}

#[tokio::test]
async fn standalone_endpoints_on_one_route_fall_through_on_a_guard_miss() {
    let router = standalone();

    let full = send(&router, Method::GET, "/deep?full", &[], "").await;
    let plain = send(&router, Method::GET, "/deep", &[], "").await;

    assert_eq!(full.1, "full");
    assert_eq!(plain.1, "plain");
}

#[tokio::test]
async fn standalone_guard_miss_is_a_client_error() {
    let (status, _) = send(&health_with_guard(), Method::GET, "/deep", &[], "").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn standalone_guarded_function_reads_headers_without_declaring_them() {
    let router = Router::new().route(echo_upper);

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
    let router = Router::new().route(general).route(general_post);

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

#[tokio::test]
async fn lone_function_may_return_data_borrowed_from_its_arguments() {
    let router = Router::new().route(echo);

    assert_eq!(
        send(&router, Method::GET, "/echo/ada", &[], "").await.1,
        "ada"
    );
}

#[tokio::test]
async fn module_variants_may_return_borrowed_data_and_name_lifetimes() {
    let router = borrowed::routes().expect("valid routes");

    assert_eq!(
        send(&router, Method::GET, "/borrowed/grace", &[], "")
            .await
            .1,
        "grace"
    );
    assert_eq!(
        send(&router, Method::GET, "/borrowed/ada", &[], "").await.1,
        "short:ada"
    );
}

#[tokio::test]
async fn lone_function_exposes_its_attribute_path() {
    assert_eq!(deep_full::PATH, "/deep");
    assert_eq!(general::PATH, "/general");
}

#[tokio::test]
async fn lone_functions_fall_through_after_a_body_guard_misses() {
    let router = Router::new().route(endpoints![priority_ticket, queued_ticket]);
    let json = [("content-type", "application/json")];

    let priority = send(
        &router,
        Method::POST,
        "/tickets",
        &json,
        r#"{"id":1,"priority":true}"#,
    )
    .await;
    let queued = send(
        &router,
        Method::POST,
        "/tickets",
        &json,
        r#"{"id":2,"priority":false}"#,
    )
    .await;

    assert_eq!(priority.1, "priority:1");
    assert_eq!(queued.1, "queued:2");
}

#[tokio::test]
async fn body_guard_miss_answers_with_the_configured_status() {
    let options = fast_http::core::DispatchOptions {
        guard_miss: StatusCode::UNPROCESSABLE_ENTITY,
        ..fast_http::core::DispatchOptions::default()
    };
    let router = Router::new().route(priority_ticket).with_options(options);

    let (status, _) = send(
        &router,
        Method::POST,
        "/tickets",
        &[("content-type", "application/json")],
        r#"{"id":3,"priority":false}"#,
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn routes_module_leaves_out_handlers_whose_cfg_is_off() {
    let router = gated::routes().expect("valid routes");

    let gone = send(&router, Method::GET, "/gated/gone", &[], "").await;
    let kept = send(&router, Method::GET, "/gated/kept", &[], "").await;
    let removed = send(&router, Method::DELETE, "/gated/kept", &[], "").await;

    assert_eq!(gone.0, StatusCode::NOT_FOUND);
    assert_eq!(kept.1, "kept");
    assert_eq!(removed.0, StatusCode::METHOD_NOT_ALLOWED);
}
