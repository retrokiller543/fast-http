//! Routes without a controller: lone functions and a state-free module, with the same typed
//! guards. Run it with `cargo run --example standalone --features macros`, then:
//!
//! ```sh
//! curl http://127.0.0.1:3000/health
//! curl http://127.0.0.1:3000/admin
//! curl -H 'x-admin: 1' http://127.0.0.1:3000/admin
//! curl http://127.0.0.1:3000/greet
//! curl 'http://127.0.0.1:3000/greet?name=ada'
//! curl http://127.0.0.1:3000/echo/ada
//! curl -H 'content-type: application/json' -d '{"text":"hi"}' http://127.0.0.1:3000/notes
//! curl 'http://127.0.0.1:3000/search?term=ada&page=2'
//! curl 'http://127.0.0.1:3000/search?term=ada%20lovelace'
//! curl http://127.0.0.1:3000/items
//! curl -H 'x-all: 1' http://127.0.0.1:3000/items
//! curl http://127.0.0.1:3000/items/0
//! curl http://127.0.0.1:3000/items/7
//! curl -H 'content-type: application/json' -d '{"item":7,"express":true}' http://127.0.0.1:3000/items/orders
//! ```
//!
//! A lone `#[get(..)]` function becomes an endpoint named after the function, and
//! `Router::route(name)` registers it at its own path. `endpoints![a, b]` makes one route of
//! several functions at the same path, which must agree on it at compile time. When a guard misses,
//! the function declines and the next one gets its turn, as `/greet` shows. Each lone function extracts
//! its own arguments and cannot hand on a body it has already read, which is the price of keeping
//! them separate. To share extraction and one body parse across the variants of a path, put the
//! functions in a `#[routes]` module: they are grouped, extracted lazily and at most once, exactly
//! like the methods of a `#[controller]`. Routes are ordered most specific first, so
//! `/items/orders` wins over `/items/{id}` whatever order they are declared in.
//!
//! `Query<T>` owns what it reads, or borrows from the request when its fields are
//! `#[serde(borrow)] Cow<'a, str>`: `/search` copies `term` only when `%20` or `+` changes it.
//!
//! `Json<T>` takes and answers exactly one format, as `/notes` does; every serde format has such a
//! wrapper (`Yaml<T>`, `Toml<T>`, ...), while `Body<T>` and `Negotiated<T>` pick from a set.
//!
//! A handler may return data borrowed from its arguments, as `echo` does: the request outlives
//! the handler call, so `-> &str` is valid and is copied into the response.
//!
//! The one rule for bodies: variants of one method and path that read the body must all take the
//! same body type. If a body can take several shapes, make it one enum and guard on its variants.

use std::{borrow::Cow, net::SocketAddr};

use http::StatusCode;
use serde::{Deserialize, Serialize};

use fast_http::{
    core::{Path, QueryParams},
    endpoints,
    format::{Json, Query},
    get, post,
    router::{PatternError, RouteList, Router},
    routes,
    server::Server,
    transport::Tcp,
};

#[get("/health")]
async fn health() -> &'static str {
    "ok"
}

#[get("/admin", when(headers.contains_key("x-admin")))]
async fn admin() -> &'static str {
    "welcome, admin"
}

#[get("/greet", when(query.contains("name")))]
async fn greet_named(query: QueryParams<'_>) -> String {
    format!("hello, {}", query.get("name").unwrap_or("stranger"))
}

#[get("/greet")]
async fn greet() -> &'static str {
    "hello, world"
}

/// The query of `/search`, whose text borrows from the request while it can.
#[derive(Deserialize)]
struct Search<'a> {
    #[serde(borrow)]
    term: Cow<'a, str>,
    page: Option<u32>,
}

#[get("/search")]
async fn search(Query(found): Query<Search<'_>>) -> String {
    let how = match found.term {
        Cow::Borrowed(_) => "borrowed",
        Cow::Owned(_) => "decoded",
    };

    format!("{} ({how}), page {}", found.term, found.page.unwrap_or(1))
}

/// A note, read and answered as JSON only.
#[derive(Deserialize, Serialize)]
struct Note {
    text: String,
}

#[post("/notes")]
async fn create_note(note: Json<Note>) -> (StatusCode, Json<Note>) {
    (StatusCode::CREATED, note)
}

#[get("/echo/{name}")]
async fn echo(Path(name): Path<&str>) -> &str {
    name
}

#[routes("/items")]
mod items {
    use std::num::NonZeroU64;

    use fast_http::{core::Path, format::Body};
    use http::StatusCode;
    use serde::Deserialize;

    /// An order for one item.
    #[derive(Deserialize)]
    pub struct Order {
        item: u64,
        express: bool,
    }

    #[get(when(headers.contains_key("x-all")))]
    async fn everything() -> &'static str {
        "all items"
    }

    #[get]
    async fn first_page() -> &'static str {
        "first page"
    }

    #[get("/{id}", when(*id == 0))]
    async fn zero(Path(id): Path<u64>) -> &'static str {
        "no such item"
    }

    #[get("/{id}")]
    async fn show(Path(id): Path<NonZeroU64>) -> String {
        format!("item {id}")
    }

    #[post("/orders", when(order.express))]
    async fn express(order: Body<Order>) -> (StatusCode, String) {
        (
            StatusCode::ACCEPTED,
            format!("express order for item {}", order.item),
        )
    }

    #[post("/orders")]
    async fn standard(order: Body<Order>) -> (StatusCode, String) {
        (
            StatusCode::ACCEPTED,
            format!("standard order for item {}", order.item),
        )
    }
}

/// The router of the example.
pub fn app() -> Result<Router<impl RouteList>, PatternError> {
    Ok(Router::new()
        .route(health)
        .route(admin)
        .route(endpoints![greet_named, greet])
        .route(echo)
        .route(search)
        .route(create_note)
        .merge(items::routes()?))
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
