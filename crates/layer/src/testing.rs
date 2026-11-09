#![allow(dead_code)]

use bytes::Bytes;
use fast_http_core::{
    ErrorHandler, ErrorResponse, Flow, HandlerError, IntoResponse, Next, ReqBody, Request,
    Response, ResponseBody, Routes, reject,
};
use fast_http_router::{Route, Router, get};
use http::{HeaderValue, StatusCode, request::Parts};
use http_body_util::{BodyExt, Full};

/// Request rejected by [`Deny`].
#[derive(Debug, thiserror::Error)]
#[error("denied")]
pub struct Denied;

impl IntoResponse for Denied {
    fn into_response(self) -> Response {
        let mut response = Response::new(ResponseBody::from("denied"));

        *response.status_mut() = StatusCode::FORBIDDEN;

        response
    }
}

impl ErrorResponse for Denied {
    fn status(&self) -> StatusCode {
        StatusCode::FORBIDDEN
    }
}

/// Rejects every request without calling the inner pipeline.
pub struct Deny;

impl Flow for Deny {
    async fn handle<B: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        req: Request<B>,
        next: N,
        errors: &H,
    ) -> Response {
        drop(next);

        reject(errors, req, Denied).await
    }
}

/// Appends its tag to `x-trace` on the way in and to `x-trace-out` on the way out.
pub struct Tag(pub &'static str);

impl Flow for Tag {
    async fn handle<B: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        req: Request<B>,
        next: N,
        _: &H,
    ) -> Response {
        let (mut parts, body) = req.into_parts();

        append(&mut parts.headers, "x-trace", self.0);

        let mut response = next.run(Request::from_parts(parts, body)).await;

        append(response.headers_mut(), "x-trace-out", self.0);

        response
    }
}

fn append(headers: &mut http::HeaderMap, name: &'static str, tag: &str) {
    let value = match headers.get(name).and_then(|value| value.to_str().ok()) {
        Some(existing) => format!("{existing},{tag}"),
        None => tag.to_owned(),
    };

    headers.insert(name, HeaderValue::from_str(&value).expect("ascii tag"));
}

/// Handler echoing the `x-trace` header it received.
pub async fn trace(parts: &Parts) -> String {
    parts
        .headers
        .get("x-trace")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-")
        .to_owned()
}

/// Error handler that marks every response it renders.
pub struct Marking;

impl ErrorHandler for Marking {
    async fn handle(&self, error: HandlerError, parts: &Parts) -> Response {
        let mut response = error.respond(parts);

        response
            .headers_mut()
            .insert("x-handled", HeaderValue::from_static("yes"));

        response
    }
}

/// A router with one `/trace` route.
pub fn router() -> Router<impl fast_http_router::RouteList> {
    Router::new((Route::new("/trace", (get(trace),)).expect("valid pattern"),))
}

/// Dispatches a GET to `uri`.
pub async fn send<R: Routes>(routes: &R, uri: &str) -> Response {
    let request = http::Request::builder()
        .uri(uri)
        .body(Full::new(Bytes::new()))
        .expect("valid request");

    routes.dispatch(request).await
}

/// Collects a response body as text.
pub async fn text(response: Response) -> String {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    String::from_utf8(bytes.to_vec()).expect("utf8")
}

/// A response header as text.
pub fn header<'a>(response: &'a Response, name: &str) -> &'a str {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-")
}
