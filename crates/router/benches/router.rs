//! Router layer: pattern matching, route lookup by position and full dispatch with extraction.

use bytes::Bytes;
use criterion::{Criterion, criterion_group, criterion_main};
use fast_http_bench_support::{Budget, bench, ready};
use fast_http_core::{Path, Routes, Segments};
use fast_http_router::{ParamBuf, Pattern, Route, Router, get, post};
use http::{Method, Request};
use http_body_util::Full;

async fn health() -> &'static str {
    "ok"
}

async fn user(Path(id): Path<u64>) -> &'static str {
    let _ = id;

    "user"
}

async fn pair(Path((org, repo)): Path<(&str, &str)>) -> &'static str {
    let _ = (org, repo);

    "pair"
}

async fn files(Path(rest): Path<Segments<'_>>) -> &'static str {
    let _ = rest;

    "files"
}

async fn create() -> &'static str {
    "created"
}

fn request(method: Method, uri: &'static str) -> Request<Full<Bytes>> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Full::new(Bytes::new()))
        .expect("valid request")
}

fn patterns(c: &mut Criterion) {
    let literal = Pattern::parse("/api/v1/health").expect("valid pattern");
    let params = Pattern::parse("/api/v1/orgs/{org}/repos/{repo}").expect("valid pattern");
    let rest = Pattern::parse("/static/{**path}").expect("valid pattern");

    bench(
        c,
        "pattern",
        "literal",
        Budget::Zero,
        || (),
        |()| {
            let mut buf = ParamBuf::new();

            literal.matches("/api/v1/health", &mut buf)
        },
    );
    bench(
        c,
        "pattern",
        "two_params",
        Budget::Zero,
        || (),
        |()| {
            let mut buf = ParamBuf::new();

            params.matches("/api/v1/orgs/rust-lang/repos/cargo", &mut buf)
        },
    );
    bench(
        c,
        "pattern",
        "rest",
        Budget::Zero,
        || (),
        |()| {
            let mut buf = ParamBuf::new();

            rest.matches("/static/css/site/main.css", &mut buf)
        },
    );
}

fn dispatch(c: &mut Criterion) {
    let router = Router::new((
        Route::new("/health", (get(health),)).expect("valid pattern"),
        Route::new("/users/{id}", (get(user),)).expect("valid pattern"),
        Route::new("/users", (post(create),)).expect("valid pattern"),
        Route::new("/orgs/{org}/repos/{repo}", (get(pair),)).expect("valid pattern"),
        Route::new("/orders", (get(health),)).expect("valid pattern"),
        Route::new("/items", (get(health),)).expect("valid pattern"),
        Route::new("/teams", (get(health),)).expect("valid pattern"),
        Route::new("/files/{**path}", (get(files),)).expect("valid pattern"),
    ));

    bench(
        c,
        "router",
        "static_route",
        Budget::Zero,
        || request(Method::GET, "/health"),
        |req| ready(router.dispatch(req)),
    );
    bench(
        c,
        "router",
        "one_typed_param",
        Budget::Zero,
        || request(Method::GET, "/users/42"),
        |req| ready(router.dispatch(req)),
    );
    bench(
        c,
        "router",
        "tuple_two_params",
        Budget::AtMost(2),
        || request(Method::GET, "/orgs/rust-lang/repos/cargo"),
        |req| ready(router.dispatch(req)),
    );
    bench(
        c,
        "router",
        "rest_segments",
        Budget::Report,
        || request(Method::GET, "/files/a/b/c/d"),
        |req| ready(router.dispatch(req)),
    );
    bench(
        c,
        "router",
        "post_literal",
        Budget::Zero,
        || request(Method::POST, "/users"),
        |req| ready(router.dispatch(req)),
    );
    bench(
        c,
        "router",
        "not_found",
        Budget::Report,
        || request(Method::GET, "/missing"),
        |req| ready(router.dispatch(req)),
    );
    bench(
        c,
        "router",
        "method_not_allowed",
        Budget::Report,
        || request(Method::DELETE, "/users"),
        |req| ready(router.dispatch(req)),
    );
}

criterion_group!(benches, patterns, dispatch);
criterion_main!(benches);
