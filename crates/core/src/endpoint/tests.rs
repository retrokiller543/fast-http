use std::{
    pin::pin,
    task::{Context, Poll, Waker},
};

use http::{Request as HttpRequest, StatusCode};
use http_body_util::Full;

use super::*;
use crate::{DefaultErrorHandler, ResponseBody};

/// Test endpoint answering with a fixed status.
struct Fixed {
    method: Method,
    phase: GuardPhase,
    head_ok: bool,
    body_needle: Option<&'static [u8]>,
    status: StatusCode,
}

impl Fixed {
    fn new(method: Method, status: StatusCode) -> Self {
        Self {
            method,
            phase: GuardPhase::Head,
            head_ok: true,
            body_needle: None,
            status,
        }
    }
}

impl Endpoint for Fixed {
    fn method(&self) -> &Method {
        &self.method
    }

    fn phase(&self) -> GuardPhase {
        self.phase
    }

    fn head_guard(&self, _: &Parts) -> bool {
        self.head_ok
    }

    fn body_guard(&self, _: &Parts, body: &Bytes) -> bool {
        self.body_needle
            .is_none_or(|needle| body.windows(needle.len()).any(|window| window == needle))
    }

    async fn call<B: ReqBody, H: ErrorHandler>(
        &self,
        _: &Parts,
        _: PathParams<'_>,
        _: B,
        _: &H,
    ) -> Response {
        let mut response = Response::new(ResponseBody::Empty);

        *response.status_mut() = self.status;

        response
    }
}

fn parts(method: Method) -> Parts {
    HttpRequest::builder()
        .method(method)
        .uri("/x")
        .body(())
        .expect("valid request")
        .into_parts()
        .0
}

fn run<S: EndpointSet>(set: &S, method: Method, body: &'static [u8]) -> Response {
    let mut cx = Context::from_waker(Waker::noop());
    let parts = parts(method);
    let future = dispatch_set(
        set,
        &parts,
        PathParams::EMPTY,
        Full::new(Bytes::from_static(body)),
        &DefaultErrorHandler,
        DispatchOptions::default(),
    );

    match pin!(future).poll(&mut cx) {
        Poll::Ready(response) => response,
        Poll::Pending => panic!("dispatch should complete without suspending"),
    }
}

#[test]
fn dispatch_set_picks_first_endpoint_whose_head_guard_passes() {
    let mut first = Fixed::new(Method::GET, StatusCode::OK);
    let second = Fixed::new(Method::GET, StatusCode::ACCEPTED);

    first.head_ok = false;

    let response = run(&(first, second), Method::GET, b"");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[test]
fn dispatch_set_get_endpoint_answers_head() {
    let set = (Fixed::new(Method::GET, StatusCode::OK),);

    let response = run(&set, Method::HEAD, b"");

    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn dispatch_set_unknown_method_returns_405_with_allow() {
    let set = (
        Fixed::new(Method::GET, StatusCode::OK),
        Fixed::new(Method::POST, StatusCode::OK),
    );

    let response = run(&set, Method::DELETE, b"");

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(response.headers()[http::header::ALLOW], "GET, HEAD, POST");
}

#[test]
fn dispatch_set_all_guards_failing_returns_guard_miss_status() {
    let mut only = Fixed::new(Method::GET, StatusCode::OK);

    only.head_ok = false;

    let response = run(&(only,), Method::GET, b"");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn dispatch_set_body_guard_selects_by_buffered_body() {
    let mut matching = Fixed::new(Method::POST, StatusCode::CREATED);
    let fallback = Fixed::new(Method::POST, StatusCode::OK);

    matching.phase = GuardPhase::Body;
    matching.body_needle = Some(b"magic");

    let set = (matching, fallback);

    assert_eq!(
        run(&set, Method::POST, b"a magic body").status(),
        StatusCode::CREATED
    );
    assert_eq!(run(&set, Method::POST, b"plain").status(), StatusCode::OK);
}
