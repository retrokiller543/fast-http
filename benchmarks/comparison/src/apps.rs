//! The same small JSON API on fast-http, axum and actix-web, with matching handlers and error
//! rendering.

use fast_http_core::{ErrorResponse, IntoResponse, Path, ResponseBody, Routes, Segments};
use fast_http_format::{Body, Negotiated};
use http::StatusCode;
use serde::{Deserialize, Serialize};

/// User returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    id: u64,
    name: String,
    email: String,
    tags: Vec<String>,
}

/// Payload for creating a user.
#[derive(Debug, Deserialize)]
pub struct NewUser {
    name: String,
    email: String,
    tags: Vec<String>,
}

/// Domain error both frameworks render as a plain-text 404.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// No user has this id.
    #[error("user {0} not found")]
    NotFound(u64),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> fast_http_core::Response {
        let mut response = fast_http_core::Response::new(ResponseBody::from(self.to_string()));

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

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        axum::response::IntoResponse::into_response((StatusCode::NOT_FOUND, self.to_string()))
    }
}

impl actix_web::ResponseError for ApiError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            Self::NotFound(_) => actix_web::http::StatusCode::NOT_FOUND,
        }
    }
}

fn find(id: u64) -> Result<User, ApiError> {
    if id == 0 {
        return Err(ApiError::NotFound(id));
    }

    Ok(User {
        id,
        name: "Ada Lovelace".to_owned(),
        email: "ada@example.com".to_owned(),
        tags: vec!["admin".to_owned(), "math".to_owned()],
    })
}

fn insert(new: NewUser) -> User {
    User {
        id: 1,
        name: new.name,
        email: new.email,
        tags: new.tags,
    }
}

/// fast-http handlers.
mod fast {
    use super::*;

    pub async fn health() -> &'static str {
        "ok"
    }

    pub async fn show(Path(id): Path<u64>) -> Result<Negotiated<User>, ApiError> {
        find(id).map(Negotiated::new)
    }

    pub async fn create(body: Body<NewUser>) -> Negotiated<User> {
        Negotiated::new(insert(body.into_inner())).status(StatusCode::CREATED)
    }

    pub async fn repo(Path((org, repo)): Path<(&str, &str)>) -> &'static str {
        let _ = (org, repo);

        "repo"
    }

    pub async fn files(Path(rest): Path<Segments<'_>>) -> &'static str {
        let _ = rest;

        "files"
    }
}

/// axum handlers.
mod ax {
    use axum::{Json, extract::Path};

    use super::*;

    pub async fn health() -> &'static str {
        "ok"
    }

    pub async fn show(Path(id): Path<u64>) -> Result<Json<User>, ApiError> {
        find(id).map(Json)
    }

    pub async fn create(Json(new): Json<NewUser>) -> (StatusCode, Json<User>) {
        (StatusCode::CREATED, Json(insert(new)))
    }

    pub async fn repo(Path((org, repo)): Path<(String, String)>) -> &'static str {
        drop((org, repo));

        "repo"
    }

    pub async fn files(Path(rest): Path<String>) -> &'static str {
        drop(rest);

        "files"
    }
}

/// actix-web handlers.
mod ac {
    use actix_web::{
        HttpResponse,
        web::{Json, Path},
    };

    use super::*;

    pub async fn health() -> &'static str {
        "ok"
    }

    pub async fn show(id: Path<u64>) -> Result<Json<User>, ApiError> {
        find(id.into_inner()).map(Json)
    }

    pub async fn create(Json(new): Json<NewUser>) -> HttpResponse {
        HttpResponse::Created().json(insert(new))
    }

    pub async fn repo(path: Path<(String, String)>) -> &'static str {
        drop(path.into_inner());

        "repo"
    }

    pub async fn files(rest: Path<String>) -> &'static str {
        drop(rest.into_inner());

        "files"
    }
}

/// The API on fast-http.
pub fn fast_http() -> impl Routes {
    use fast_http_router::{Route, Router};

    Router::new()
        .route(
            Route::new("/health")
                .expect("valid route")
                .get(fast::health),
        )
        .route(
            Route::new("/users/{id}")
                .expect("valid route")
                .get(fast::show),
        )
        .route(
            Route::new("/users")
                .expect("valid route")
                .post(fast::create),
        )
        .route(
            Route::new("/orgs/{org}/repos/{repo}")
                .expect("valid route")
                .get(fast::repo),
        )
        .route(
            Route::new("/files/{**path}")
                .expect("valid route")
                .get(fast::files),
        )
}

/// The API on axum.
pub fn axum() -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/health", get(ax::health))
        .route("/users/{id}", get(ax::show))
        .route("/users", post(ax::create))
        .route("/orgs/{org}/repos/{repo}", get(ax::repo))
        .route("/files/{*path}", get(ax::files))
}

/// The API on actix-web, for `App::new().configure(apps::actix)`.
pub fn actix(config: &mut actix_web::web::ServiceConfig) {
    use actix_web::web::{get, post};

    config
        .route("/health", get().to(ac::health))
        .route("/users/{id}", get().to(ac::show))
        .route("/users", post().to(ac::create))
        .route("/orgs/{org}/repos/{repo}", get().to(ac::repo))
        .route("/files/{path:.*}", get().to(ac::files));
}
