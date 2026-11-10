#![cfg(feature = "macros")]

use std::sync::Arc;

use bytes::Bytes;
use fast_http::{
    controller,
    core::{HeadGuard, Path, Routes},
    router::Router,
};
use http::{Method, Request, StatusCode, request::Parts};
use http_body_util::{BodyExt, Full};

struct Chat {
    greeting: &'static str,
}

struct Admin;

impl HeadGuard for Admin {
    type Output = ();

    fn check(parts: &Parts) -> Option<()> {
        parts.headers.contains_key("x-admin").then_some(())
    }
}

#[controller("/chat")]
impl Chat {
    #[post(when(query.stream = true))]
    async fn stream(&self) -> String {
        format!("{} stream", self.greeting)
    }

    #[post(when(query.stream = false))]
    async fn buffered(&self) -> &'static str {
        "buffered"
    }

    #[post]
    async fn fallback(&self) -> &'static str {
        "default"
    }

    #[get("/health")]
    async fn health() -> &'static str {
        "ok"
    }

    #[get("/secret", when(guard(Admin)))]
    async fn secret(&self) -> &'static str {
        "secret"
    }

    #[get("/mode", when(header.x_mode = "fast", not(query.debug)))]
    async fn mode(&self) -> &'static str {
        "fast"
    }

    #[get("/who/{id}")]
    #[get("/user/{id}")]
    async fn show(&self, Path(id): Path<u64>, parts: &Parts) -> String {
        format!("{}:{id}:{}", self.greeting, parts.uri.path())
    }
}

struct Ping;

#[controller]
impl Ping {
    #[get("/ping")]
    async fn ping(&self) -> &'static str {
        "pong"
    }
}

struct Broken;

#[controller]
impl Broken {
    #[get("/a/**/b")]
    async fn broken(&self) -> &'static str {
        "never"
    }
}

fn chat() -> Router<impl fast_http::router::RouteList> {
    Arc::new(Chat { greeting: "hi" })
        .routes()
        .expect("valid routes")
}

async fn send<R: Routes>(
    router: &R,
    method: Method,
    uri: &str,
    header: Option<(&str, &str)>,
) -> (StatusCode, String) {
    let mut builder = Request::builder().method(method).uri(uri);

    if let Some((name, value)) = header {
        builder = builder.header(name, value);
    }

    let request = builder
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
async fn controller_methods_see_their_state_and_extractors() {
    let (status, body) = send(&chat(), Method::GET, "/chat/who/7", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "hi:7:/chat/who/7");
}

#[tokio::test]
async fn controller_second_attribute_adds_a_second_path() {
    let (_, body) = send(&chat(), Method::GET, "/chat/user/9", None).await;

    assert_eq!(body, "hi:9:/chat/user/9");
}

#[tokio::test]
async fn controller_guard_selects_the_stream_variant() {
    let (_, body) = send(&chat(), Method::POST, "/chat?stream=true", None).await;

    assert_eq!(body, "hi stream");
}

#[tokio::test]
async fn controller_guard_selects_the_buffered_variant() {
    let (_, body) = send(&chat(), Method::POST, "/chat?stream=false", None).await;

    assert_eq!(body, "buffered");
}

#[tokio::test]
async fn controller_unguarded_variant_answers_the_rest() {
    let (_, body) = send(&chat(), Method::POST, "/chat", None).await;

    assert_eq!(body, "default");
}

#[tokio::test]
async fn controller_method_without_receiver_is_a_route() {
    let (_, body) = send(&chat(), Method::GET, "/chat/health", None).await;

    assert_eq!(body, "ok");
}

#[tokio::test]
async fn controller_head_guard_type_admits_matching_requests() {
    let (status, body) = send(&chat(), Method::GET, "/chat/secret", Some(("x-admin", "1"))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "secret");
}

#[tokio::test]
async fn controller_head_guard_type_rejects_other_requests() {
    let (status, _) = send(&chat(), Method::GET, "/chat/secret", None).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn controller_combined_conditions_must_all_hold() {
    let fast = Some(("x-mode", "fast"));

    assert_eq!(
        send(&chat(), Method::GET, "/chat/mode", fast).await.0,
        StatusCode::OK
    );
    assert_eq!(
        send(&chat(), Method::GET, "/chat/mode?debug", fast).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(&chat(), Method::GET, "/chat/mode", Some(("x-mode", "slow")))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn controllers_merge_into_one_router() {
    let router = Router::new()
        .merge(chat())
        .merge(Arc::new(Ping).routes().expect("valid routes"));

    assert_eq!(send(&router, Method::GET, "/ping", None).await.1, "pong");
    assert_eq!(
        send(&router, Method::GET, "/chat/health", None).await.1,
        "ok"
    );
}

#[test]
fn routes_reports_a_malformed_pattern() {
    let result = Arc::new(Broken).routes();

    assert!(result.is_err());
}
