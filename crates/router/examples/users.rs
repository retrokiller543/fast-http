//! Sends mocked requests through the router to plain `async fn` handlers and prints the responses.

use bytes::Bytes;
use fast_http_core::{ErrorResponse, IntoResponse, Path, Response, ResponseBody, Routes};
use fast_http_format::{Body, Negotiated};
use http::{Method, Request, StatusCode, header};
use http_body_util::{BodyExt, Full};
use serde::{Deserialize, Serialize};

use fast_http_router::{Route, Router};

/// A user as stored and as returned.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
}

/// Payload for creating a user.
#[derive(Debug, Deserialize)]
struct NewUser {
    name: String,
}

/// Application error that renders itself; the router never needs to know about it.
#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("user {0} not found")]
    NotFound(u64),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = Response::new(ResponseBody::from(self.to_string()));

        *response.status_mut() = self.status();

        response
    }
}

impl ErrorResponse for ApiError {
    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
        }
    }
}

async fn show(Path(id): Path<u64>) -> Result<Negotiated<User>, ApiError> {
    if id == 0 {
        return Err(ApiError::NotFound(id));
    }

    Ok(Negotiated::new(User {
        id,
        name: format!("user-{id}"),
    }))
}

async fn create(body: Body<NewUser>) -> Negotiated<User> {
    let NewUser { name } = body.into_inner();

    Negotiated::new(User { id: 1, name }).status(StatusCode::CREATED)
}

async fn send(router: &impl Routes, method: Method, uri: &str, content: Option<&'static str>) {
    let request = Request::builder()
        .method(method.clone())
        .uri(uri)
        .header(header::ACCEPT, "application/json")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from_static(
            content.unwrap_or("").as_bytes(),
        )))
        .expect("valid request");
    let response = router.dispatch(request).await;
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    println!(
        "{method} {uri} -> {status} {}",
        String::from_utf8_lossy(&body)
    );
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let router = Router::new()
        .route(Route::new("/users/{id}").expect("valid pattern").get(show))
        .route(Route::new("/users").expect("valid pattern").post(create));

    send(&router, Method::GET, "/users/7", None).await;
    send(&router, Method::GET, "/users/0", None).await;
    send(&router, Method::GET, "/users/abc", None).await;
    send(&router, Method::POST, "/users", Some(r#"{"name":"ada"}"#)).await;
    send(&router, Method::POST, "/users", Some("not json")).await;
    send(&router, Method::DELETE, "/users", None).await;
    send(&router, Method::GET, "/nope", None).await;
}
