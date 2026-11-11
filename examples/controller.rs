//! Controllers with typed guards. Run it with `cargo run --example controller --features macros`,
//! then:
//!
//! ```sh
//! curl http://127.0.0.1:3000/items
//! curl -H 'x-staff: 1' http://127.0.0.1:3000/items
//! curl 'http://127.0.0.1:3000/items/search?sort=name'
//! curl http://127.0.0.1:3000/items/7
//! curl -H 'x-api-version: 2' http://127.0.0.1:3000/items/7
//! curl -H 'content-type: application/json' -d '{"item":7,"express":true}' http://127.0.0.1:3000/orders
//! curl -H 'content-type: application/json' -d '{"item":7,"express":false}' http://127.0.0.1:3000/orders
//! ```
//!
//! A guard in `when(..)` is an ordinary Rust expression over the method's own parameters, so the
//! editor highlights and checks it like any other code. Parameters are borrowed in a guard:
//! `headers` is a `&HeaderMap`, `order` a `&Order`.
//!
//! Work happens only when it is needed and at most once. Within a variant the arguments a guard
//! reads are extracted first, so a header check runs before any body is parsed, and when several
//! variants take the same argument type it is extracted once and moved into whichever variant
//! wins. Variants are tried in order, so put cheap and specific guards first and the unguarded
//! default last.

use std::{net::SocketAddr, sync::Arc};

use fast_http::{
    controller,
    core::{Path, QueryParams},
    format::{Body, Negotiated},
    router::{PatternError, RouteList, Router},
    server::Server,
    transport::Tcp,
};
use http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};

/// The state of the items controller; handlers reach it through `&self`.
struct Library {
    name: &'static str,
}

/// A listing of items.
#[derive(Serialize)]
struct Listing {
    library: &'static str,
    scope: &'static str,
    items: Vec<u64>,
}

/// An item as returned, tagged with the API version that produced it.
#[derive(Serialize)]
struct Item {
    id: u64,
    version: &'static str,
}

#[controller("/items")]
impl Library {
    #[get(when(headers.contains_key("x-staff")))]
    async fn everything(&self, headers: &HeaderMap) -> Negotiated<Listing> {
        Negotiated::new(Listing {
            library: self.name,
            scope: "all",
            items: (1..=100).collect(),
        })
    }

    #[get]
    async fn first_page(&self) -> Negotiated<Listing> {
        Negotiated::new(Listing {
            library: self.name,
            scope: "first page",
            items: (1..=10).collect(),
        })
    }

    #[get("/search", when(query.get("sort") == Some("name")))]
    async fn by_name(&self, query: QueryParams<'_>) -> Negotiated<Listing> {
        Negotiated::new(Listing {
            library: self.name,
            scope: "by name",
            items: (1..=10).rev().collect(),
        })
    }

    #[get("/search")]
    async fn by_id(&self) -> Negotiated<Listing> {
        Negotiated::new(Listing {
            library: self.name,
            scope: "by id",
            items: (1..=10).collect(),
        })
    }

    #[get("/{id}", when(headers.get("x-api-version").is_some_and(|version| version == "2")))]
    async fn show_v2(&self, Path(id): Path<u64>, headers: &HeaderMap) -> Negotiated<Item> {
        Negotiated::new(Item { id, version: "v2" })
    }

    #[get("/{id}")]
    async fn show_v1(&self, Path(id): Path<u64>) -> Negotiated<Item> {
        Negotiated::new(Item { id, version: "v1" })
    }
}

/// An order for one item.
#[derive(Deserialize)]
struct Order {
    item: u64,
    express: bool,
}

/// The orders controller.
struct Orders;

#[controller("/orders")]
impl Orders {
    #[post(when(order.express))]
    async fn express(&self, Body { value: order, .. }: Body<Order>) -> (StatusCode, String) {
        (
            StatusCode::ACCEPTED,
            format!("express order for item {}", order.item),
        )
    }

    #[post]
    async fn standard(&self, order: Body<Order>) -> (StatusCode, String) {
        (
            StatusCode::ACCEPTED,
            format!("standard order for item {}", order.value.item),
        )
    }
}

/// The router of the example.
pub fn app() -> Result<Router<impl RouteList>, PatternError> {
    let library = Arc::new(Library { name: "city" });

    Ok(Router::new()
        .merge(library.routes()?)
        .merge(Arc::new(Orders).routes()?))
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
