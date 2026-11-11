//! A runnable server for manual testing. Run it with `just serve`, then:
//!
//! ```sh
//! curl http://127.0.0.1:3000/users/7
//! curl --http2-prior-knowledge http://127.0.0.1:3000/users/7
//! curl http://127.0.0.1:3000/users/0
//! curl -X POST -H 'content-type: application/json' -d '{"name":"ada"}' http://127.0.0.1:3000/users
//! curl http://127.0.0.1:3000/peer
//! ```
//!
//! Serves plain HTTP/1.1 and prior-knowledge h2 on port 3000, as behind a TLS-terminating
//! proxy. Ctrl-C drains in-flight requests and exits.

use fast_http_core::{
    ErrorHandler, ErrorResponse, HandlerError, IntoResponse, Path, PeerAddr, Response, ResponseBody,
};
use fast_http_format::{Body, Negotiated};
use fast_http_router::{Route, Router};
use fast_http_server::Server;
use fast_http_transport::Tcp;
use http::StatusCode;
use http::request::Parts;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use type_match::type_match;

/// A user as returned.
#[derive(Debug, Serialize)]
struct User {
    id: u64,
    name: String,
}

/// Payload for creating a user.
#[derive(Debug, Deserialize)]
struct NewUser {
    name: String,
}

/// Application error that renders itself.
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

#[derive(Debug)]
struct TestError {
    test_error: Cow<'static, str>,
}

impl Display for TestError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.test_error)
    }
}

impl std::error::Error for TestError {}

impl IntoResponse for TestError {
    fn into_response(self) -> Response {
        Response::builder()
            .status(self.status())
            .header("content-type", "application/json")
            .header("x-error", self.test_error.to_string())
            .body(ResponseBody::from(
                serde_json::json!({ "test_error": self.test_error }).to_string(),
            ))
            .unwrap()
    }
}

impl ErrorResponse for TestError {
    fn status(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

async fn test(Path(name): Path<&str>) -> Result<Negotiated<User>, TestError> {
    Err(TestError {
        test_error: Cow::from(name.to_string()),
    })
}

async fn peer(PeerAddr(addr): PeerAddr) -> String {
    format!("hello {addr}\n")
}

struct GlobalErrorHandler;

impl ErrorHandler for GlobalErrorHandler {
    async fn handle(&self, error: HandlerError, parts: &Parts) -> Response {
        type_match! {
            match &error {
                test_error @ TestError => {
                    eprintln!("Got a TestError: {test_error} (from {parts:?})");
                }
                _ => {
                    eprintln!("error: {error} (from {parts:?})");
                }
            }
        }

        error.respond(parts)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let routes = Router::new()
        .route(Route::new("/users/{id}")?.get(show))
        .route(Route::new("/users")?.post(create))
        .route(Route::new("/peer")?.get(peer))
        .route(Route::new("/test/{error}")?.get(test))
        .with_error_handler(GlobalErrorHandler);
    let server = Server::bind(&Tcp::new(), SocketAddr::from(([127, 0, 0, 1], 3000))).await?;

    println!("listening on http://{}", server.local_addr()?);

    server
        .graceful_shutdown(async {
            if let Err(error) = tokio::signal::ctrl_c().await {
                eprintln!("no Ctrl-C handler ({error}); stop the process to exit");

                return std::future::pending().await;
            }

            println!("draining");
        })
        .serve(routes)
        .await;

    Ok(())
}
