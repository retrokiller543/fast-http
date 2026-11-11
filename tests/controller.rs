#![cfg(feature = "macros")]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use bytes::Bytes;
use fast_http::{
    controller,
    core::{FromRequest, Path, QueryParams, Rejection, ReqBody, Routes, buffer},
    format::Query,
    router::Router,
};
use http::{HeaderMap, Method, Request, StatusCode, request::Parts};
use http_body_util::{BodyExt, Full};
use serde::Deserialize;

static FIRST_PARSES: AtomicUsize = AtomicUsize::new(0);
static SECOND_PARSES: AtomicUsize = AtomicUsize::new(0);

async fn text_of<B: ReqBody>(body: B, counter: &AtomicUsize) -> Result<String, Rejection> {
    counter.fetch_add(1, Ordering::SeqCst);

    let bytes = buffer(body, 1024).await?;

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

struct First(String);

impl FromRequest for First {
    type Rejection = Rejection;

    async fn from_request<B: ReqBody>(_: &Parts, body: B) -> Result<Self, Rejection> {
        text_of(body, &FIRST_PARSES).await.map(Self)
    }
}

struct Second(String);

impl FromRequest for Second {
    type Rejection = Rejection;

    async fn from_request<B: ReqBody>(_: &Parts, body: B) -> Result<Self, Rejection> {
        text_of(body, &SECOND_PARSES).await.map(Self)
    }
}

#[derive(Deserialize)]
struct Params {
    stream: Option<bool>,
}

struct Chat {
    greeting: &'static str,
}

#[controller("/chat")]
impl Chat {
    #[post(when(params.stream == Some(true)))]
    async fn stream(&self, Query(params): Query<Params>) -> String {
        format!("{} stream", self.greeting)
    }

    #[post(when(query.get("stream") == Some("false")))]
    async fn buffered(&self, query: QueryParams<'_>) -> &'static str {
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

    #[get("/mode", when(headers.get("x-mode").is_some_and(|mode| mode == "fast"), !query.contains("debug")))]
    async fn mode(&self, headers: &HeaderMap, query: QueryParams<'_>) -> &'static str {
        "fast"
    }

    #[get("/who/{id}", when(*id > 0))]
    #[get("/user/{id}")]
    async fn show(&self, Path(id): Path<u64>, parts: &Parts) -> String {
        format!("{}:{id}:{}", self.greeting, parts.uri.path())
    }

    #[post("/echo", when(headers.contains_key("x-fast")))]
    async fn echo_fast(&self, headers: &HeaderMap, body: Second) -> String {
        format!("fast:{}", body.0)
    }

    #[post("/echo")]
    async fn echo(&self, body: First) -> String {
        format!("slow:{}", body.0)
    }

    #[post("/ask", when(body.0 == "fast"))]
    async fn ask_fast(&self, body: First) -> String {
        format!("fast:{}", body.0)
    }

    #[post("/ask")]
    async fn ask(&self, body: First) -> String {
        format!("any:{}", body.0)
    }

    #[post("/two", when(first.0 == "a"))]
    async fn two_a(&self, first: First) -> String {
        format!("a:{}", first.0)
    }

    #[post("/two")]
    async fn two_b(&self, second: Second) -> String {
        format!("b:{}", second.0)
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
    body: &'static str,
) -> (StatusCode, String) {
    let mut builder = Request::builder().method(method).uri(uri);

    if let Some((name, value)) = header {
        builder = builder.header(name, value);
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

async fn post(uri: &str, header: Option<(&str, &str)>, body: &'static str) -> (StatusCode, String) {
    send(&chat(), Method::POST, uri, header, body).await
}

#[tokio::test]
async fn controller_methods_see_their_state_and_extractors() {
    let (status, body) = send(&chat(), Method::GET, "/chat/who/7", None, "").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "hi:7:/chat/who/7");
}

#[tokio::test]
async fn controller_guard_can_read_a_path_parameter() {
    let (status, _) = send(&chat(), Method::GET, "/chat/who/0", None, "").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn controller_second_attribute_adds_a_second_path() {
    let (_, body) = send(&chat(), Method::GET, "/chat/user/9", None, "").await;

    assert_eq!(body, "hi:9:/chat/user/9");
}

#[tokio::test]
async fn controller_guard_on_a_typed_query_selects_the_stream_variant() {
    assert_eq!(post("/chat?stream=true", None, "").await.1, "hi stream");
}

#[tokio::test]
async fn controller_guard_on_a_zero_copy_query_selects_the_buffered_variant() {
    assert_eq!(post("/chat?stream=false", None, "").await.1, "buffered");
}

#[tokio::test]
async fn controller_unguarded_variant_answers_the_rest() {
    assert_eq!(post("/chat", None, "").await.1, "default");
}

#[tokio::test]
async fn controller_malformed_typed_query_is_an_error_not_a_fallthrough() {
    let (status, _) = post("/chat?stream=maybe", None, "").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn controller_method_without_receiver_is_a_route() {
    assert_eq!(
        send(&chat(), Method::GET, "/chat/health", None, "").await.1,
        "ok"
    );
}

#[tokio::test]
async fn controller_combined_guards_must_all_hold() {
    let fast = Some(("x-mode", "fast"));
    let get = |uri: &'static str, header| async move {
        send(&chat(), Method::GET, uri, header, "").await.0
    };

    assert_eq!(get("/chat/mode", fast).await, StatusCode::OK);
    assert_eq!(get("/chat/mode?debug", fast).await, StatusCode::BAD_REQUEST);
    assert_eq!(
        get("/chat/mode", Some(("x-mode", "slow"))).await,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn controller_head_guard_failing_never_parses_the_guarded_variants_body() {
    FIRST_PARSES.store(0, Ordering::SeqCst);
    SECOND_PARSES.store(0, Ordering::SeqCst);

    let (_, text) = post("/chat/echo", None, "payload").await;

    assert_eq!(text, "slow:payload");
    assert_eq!(FIRST_PARSES.load(Ordering::SeqCst), 1);
    assert_eq!(SECOND_PARSES.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn controller_head_guard_passing_parses_only_the_matching_variants_body() {
    FIRST_PARSES.store(0, Ordering::SeqCst);
    SECOND_PARSES.store(0, Ordering::SeqCst);

    let (_, text) = post("/chat/echo", Some(("x-fast", "1")), "payload").await;

    assert_eq!(text, "fast:payload");
    assert_eq!(FIRST_PARSES.load(Ordering::SeqCst), 0);
    assert_eq!(SECOND_PARSES.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn controller_body_guard_matches_on_the_extracted_value() {
    FIRST_PARSES.store(0, Ordering::SeqCst);

    assert_eq!(post("/chat/ask", None, "fast").await.1, "fast:fast");
    assert_eq!(FIRST_PARSES.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn controller_variants_sharing_a_body_type_extract_it_once() {
    FIRST_PARSES.store(0, Ordering::SeqCst);

    assert_eq!(post("/chat/ask", None, "slow").await.1, "any:slow");
    assert_eq!(FIRST_PARSES.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn controller_variants_with_different_body_types_share_one_buffered_body() {
    FIRST_PARSES.store(0, Ordering::SeqCst);
    SECOND_PARSES.store(0, Ordering::SeqCst);

    assert_eq!(post("/chat/two", None, "a").await.1, "a:a");
    assert_eq!(post("/chat/two", None, "b").await.1, "b:b");
    assert_eq!(SECOND_PARSES.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn controllers_merge_into_one_router() {
    let router = Router::new()
        .merge(chat())
        .merge(Arc::new(Ping).routes().expect("valid routes"));

    assert_eq!(
        send(&router, Method::GET, "/ping", None, "").await.1,
        "pong"
    );
    assert_eq!(
        send(&router, Method::GET, "/chat/health", None, "").await.1,
        "ok"
    );
}

#[test]
fn routes_reports_a_malformed_pattern() {
    assert!(Arc::new(Broken).routes().is_err());
}
