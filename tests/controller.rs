#![cfg(feature = "macros")]

use std::{
    convert::Infallible,
    num::NonZeroU64,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use bytes::Bytes;
use fast_http::{
    controller,
    core::{FromParts, FromRequest, Path, QueryParams, Rejection, ReqBody, Routes, buffer},
    format::{Body, Query},
    router::Router,
};
use http::{HeaderMap, Method, Request, StatusCode, request::Parts};
use http_body_util::{BodyExt, Full};
use serde::Deserialize;

static FIRST_PARSES: AtomicUsize = AtomicUsize::new(0);

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

/// A body that can take several shapes: one type, grouped by its variants.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Job {
    Fast { units: u32 },
    Slow { units: u32 },
}

impl Job {
    fn units(&self) -> u32 {
        match self {
            Self::Fast { units } | Self::Slow { units } => *units,
        }
    }
}

#[derive(Deserialize)]
struct Params {
    stream: Option<bool>,
}

/// A head extractor that is neither a `HeaderMap` nor a `QueryParams`.
struct Tag(&'static str);

impl FromParts for Tag {
    type Rejection = Infallible;

    fn from_parts(parts: &Parts) -> Result<Self, Infallible> {
        Ok(Self(if parts.headers.contains_key("x-tag") {
            "tagged"
        } else {
            "plain"
        }))
    }
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
    async fn echo_fast(&self, body: First) -> String {
        format!("fast:{}", body.0)
    }

    #[post("/echo")]
    async fn echo(&self) -> &'static str {
        "slow"
    }

    #[post("/ask", when(body.0 == "fast"))]
    async fn ask_fast(&self, body: First) -> String {
        format!("fast:{}", body.0)
    }

    #[post("/ask")]
    async fn ask(&self, body: First) -> String {
        format!("any:{}", body.0)
    }

    #[get("/precedence/q", when(query.0 == "tagged"))]
    async fn query_param_wins(&self, query: Tag) -> &'static str {
        "tagged"
    }

    #[get("/precedence/q")]
    async fn query_param_default(&self) -> &'static str {
        "plain"
    }

    #[get("/precedence/h", when(headers.0 == "tagged"))]
    async fn headers_param_wins(&self, headers: Tag) -> &'static str {
        "tagged"
    }

    #[get("/precedence/h")]
    async fn headers_param_default(&self) -> &'static str {
        "plain"
    }

    #[get("/ambient", when(headers.contains_key("x-a"), query.get("m") == Some("1")))]
    async fn ambient(&self) -> &'static str {
        "ambient"
    }

    #[get("/ambient")]
    async fn ambient_default(&self) -> &'static str {
        "default"
    }

    #[get("/path/{x}", when(parts.uri.path().ends_with("/7")))]
    async fn seven(&self, parts: &Parts) -> &'static str {
        "seven"
    }

    #[get("/path/{x}")]
    async fn other(&self) -> &'static str {
        "other"
    }

    #[get("/nz/{id}", when(*id == 0))]
    async fn nz_bad(&self, Path(id): Path<u64>) -> &'static str {
        "zero"
    }

    #[get("/nz/{id}")]
    async fn nz(&self, Path(id): Path<NonZeroU64>) -> String {
        format!("nz:{id}")
    }

    #[post("/static", when(item.0 == "x"))]
    async fn stat(item: First) -> String {
        format!("static:{}", item.0)
    }

    #[post("/job", when(matches!(job.value, Job::Fast { .. })))]
    async fn job_fast(&self, job: Body<Job>) -> String {
        format!("fast:{}", job.value.units())
    }

    #[post("/job", when(matches!(job.value, Job::Slow { units } if units > 10)))]
    async fn job_big(&self, job: Body<Job>) -> String {
        format!("big:{}", job.value.units())
    }

    #[post("/job")]
    async fn job_slow(&self, job: Body<Job>) -> String {
        format!("slow:{}", job.value.units())
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
async fn controller_head_guard_failing_never_parses_the_body() {
    FIRST_PARSES.store(0, Ordering::SeqCst);

    let (_, text) = post("/chat/echo", None, "payload").await;

    assert_eq!(text, "slow");
    assert_eq!(FIRST_PARSES.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn controller_head_guard_passing_parses_the_body_once() {
    FIRST_PARSES.store(0, Ordering::SeqCst);

    let (_, text) = post("/chat/echo", Some(("x-fast", "1")), "payload").await;

    assert_eq!(text, "fast:payload");
    assert_eq!(FIRST_PARSES.load(Ordering::SeqCst), 1);
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

async fn job(body: &'static str) -> (StatusCode, String) {
    post(
        "/chat/job",
        Some(("content-type", "application/json")),
        body,
    )
    .await
}

#[tokio::test]
async fn controller_one_body_enum_is_grouped_by_guards_on_its_variants() {
    assert_eq!(job(r#"{"kind":"fast","units":3}"#).await.1, "fast:3");
    assert_eq!(job(r#"{"kind":"slow","units":30}"#).await.1, "big:30");
    assert_eq!(job(r#"{"kind":"slow","units":4}"#).await.1, "slow:4");
}

#[tokio::test]
async fn controller_a_body_that_fits_no_enum_variant_is_a_client_error() {
    assert!(
        job(r#"{"kind":"unknown","units":1}"#)
            .await
            .0
            .is_client_error()
    );
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

#[tokio::test]
async fn controller_guard_reads_headers_and_query_without_declaring_them() {
    let ok = send(
        &chat(),
        Method::GET,
        "/chat/ambient?m=1",
        Some(("x-a", "1")),
        "",
    )
    .await;
    let no_header = send(&chat(), Method::GET, "/chat/ambient?m=1", None, "").await;
    let no_query = send(
        &chat(),
        Method::GET,
        "/chat/ambient",
        Some(("x-a", "1")),
        "",
    )
    .await;

    assert_eq!(ok.1, "ambient");
    assert_eq!(no_header.1, "default");
    assert_eq!(no_query.1, "default");
}

#[tokio::test]
async fn controller_guard_reads_the_whole_head_through_a_parts_parameter() {
    assert_eq!(
        send(&chat(), Method::GET, "/chat/path/7", None, "").await.1,
        "seven"
    );
    assert_eq!(
        send(&chat(), Method::GET, "/chat/path/8", None, "").await.1,
        "other"
    );
}

#[tokio::test]
async fn controller_variants_may_take_the_same_path_parameter_as_different_types() {
    assert_eq!(
        send(&chat(), Method::GET, "/chat/nz/0", None, "").await.1,
        "zero"
    );
    assert_eq!(
        send(&chat(), Method::GET, "/chat/nz/5", None, "").await.1,
        "nz:5"
    );
}

#[tokio::test]
async fn controller_method_without_receiver_can_carry_a_body_guard() {
    FIRST_PARSES.store(0, Ordering::SeqCst);

    assert_eq!(post("/chat/static", None, "x").await.1, "static:x");
    assert_eq!(
        post("/chat/static", None, "y").await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn controller_a_parameter_named_like_a_reserved_view_takes_precedence() {
    let get = |uri: &'static str, header| async move {
        send(&chat(), Method::GET, uri, header, "").await.0
    };

    assert_eq!(
        get("/chat/mode", Some(("x-mode", "fast"))).await,
        StatusCode::OK
    );
    assert_eq!(
        get("/chat/mode", Some(("x-mode", "other"))).await,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn controller_a_parameter_named_query_wins_whatever_its_type() {
    let get = |header| async move {
        send(&chat(), Method::GET, "/chat/precedence/q", header, "")
            .await
            .1
    };

    assert_eq!(get(Some(("x-tag", "1"))).await, "tagged");
    assert_eq!(get(None).await, "plain");
}

#[tokio::test]
async fn controller_a_parameter_named_headers_wins_whatever_its_type() {
    let get = |header| async move {
        send(&chat(), Method::GET, "/chat/precedence/h", header, "")
            .await
            .1
    };

    assert_eq!(get(Some(("x-tag", "1"))).await, "tagged");
    assert_eq!(get(None).await, "plain");
}

#[derive(Deserialize)]
struct Search<'a> {
    #[serde(borrow)]
    term: std::borrow::Cow<'a, str>,
}

#[fast_http::get("/search")]
async fn search(Query(found): Query<Search<'_>>) -> String {
    format!(
        "{}:{}",
        found.term,
        matches!(found.term, std::borrow::Cow::Borrowed(_))
    )
}

#[tokio::test]
async fn raw_query_hands_a_handler_borrowed_fields() {
    let router = Router::new().route(search);

    let plain = send(&router, Method::GET, "/search?term=ada", None, "").await;
    let encoded = send(&router, Method::GET, "/search?term=a%20b", None, "").await;

    assert_eq!(plain.1, "ada:true");
    assert_eq!(encoded.1, "a b:false");
}
