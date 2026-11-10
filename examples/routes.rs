//! Bare routes without macros, with header and body guards. Run it with
//! `cargo run --example routes`, then:
//!
//! ```sh
//! curl http://127.0.0.1:3000/items/7
//! curl -H 'x-api-version: 2' http://127.0.0.1:3000/items/7
//! curl -H 'content-type: application/json' -d '{"kind":"book","title":"Dune"}' http://127.0.0.1:3000/items
//! curl -H 'content-type: application/json' -d '{"kind":"movie","title":"Alien","minutes":117}' http://127.0.0.1:3000/items
//! curl -H 'content-type: application/json' -d '{"kind":"game"}' http://127.0.0.1:3000/items
//! ```
//!
//! Endpoints that share a path are tried in the order they are added, so the unguarded one goes
//! last. A head guard sees the request line and headers. A body guard also sees the body: the
//! server buffers it once and the selected handler gets the same bytes.

use std::net::SocketAddr;

use bytes::Bytes;
use fast_http::{
    core::{Path, guard::header_eq},
    format::{Body, Negotiated},
    router::{PatternError, Route, RouteList, Router, get, post},
    server::Server,
    transport::Tcp,
};
use http::{StatusCode, request::Parts};
use serde::{Deserialize, Serialize};

/// An item as returned, tagged with the API version that produced it.
#[derive(Serialize)]
struct Item {
    id: u64,
    version: &'static str,
}

/// The part of a creation request every kind shares, read by the body guards.
#[derive(Deserialize)]
struct Probe {
    kind: String,
}

/// Payload for creating a book.
#[derive(Deserialize)]
struct NewBook {
    title: String,
}

/// Payload for creating a movie.
#[derive(Deserialize)]
struct NewMovie {
    title: String,
    minutes: u32,
}

async fn show_v2(Path(id): Path<u64>) -> Negotiated<Item> {
    Negotiated::new(Item { id, version: "v2" })
}

async fn show_v1(Path(id): Path<u64>) -> Negotiated<Item> {
    Negotiated::new(Item { id, version: "v1" })
}

async fn create_book(body: Body<NewBook>) -> (StatusCode, String) {
    (
        StatusCode::CREATED,
        format!("book {}", body.into_inner().title),
    )
}

async fn create_movie(body: Body<NewMovie>) -> (StatusCode, String) {
    let NewMovie { title, minutes } = body.into_inner();

    (
        StatusCode::CREATED,
        format!("movie {title} ({minutes} min)"),
    )
}

async fn create_other() -> (StatusCode, &'static str) {
    (StatusCode::UNPROCESSABLE_ENTITY, "unknown kind")
}

fn wants_v2(parts: &Parts) -> bool {
    header_eq(parts, "x-api-version", "2")
}

fn has_kind(body: &Bytes, kind: &str) -> bool {
    serde_json::from_slice::<Probe>(body).is_ok_and(|probe| probe.kind == kind)
}

fn is_book(_: &Parts, body: &Bytes) -> bool {
    has_kind(body, "book")
}

fn is_movie(_: &Parts, body: &Bytes) -> bool {
    has_kind(body, "movie")
}

/// The router of the example.
pub fn app() -> Result<Router<impl RouteList>, PatternError> {
    Ok(Router::new()
        .route(
            Route::new("/items/{id}")?
                .endpoint(get(show_v2).guard(wants_v2))
                .endpoint(get(show_v1)),
        )
        .route(
            Route::new("/items")?
                .endpoint(post(create_book).body_guard(is_book))
                .endpoint(post(create_movie).body_guard(is_movie))
                .endpoint(post(create_other)),
        ))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::bind(&Tcp::new(), SocketAddr::from(([127, 0, 0, 1], 3000))).await?;

    println!("listening on http://{}", server.local_addr()?);

    server
        .graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .serve(app()?)
        .await;

    Ok(())
}
