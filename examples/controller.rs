//! A controller with header guards, mixed with a bare route that has a body guard. Run it with
//! `cargo run --example controller --features macros`, then:
//!
//! ```sh
//! curl http://127.0.0.1:3000/items
//! curl -H 'x-staff: 1' http://127.0.0.1:3000/items
//! curl http://127.0.0.1:3000/items/7
//! curl -H 'x-api-version: 2' http://127.0.0.1:3000/items/7
//! curl -H 'content-type: application/json' -d '{"item":7,"express":true}' http://127.0.0.1:3000/orders
//! curl -H 'content-type: application/json' -d '{"item":7,"express":false}' http://127.0.0.1:3000/orders
//! ```
//!
//! `when(..)` covers the request head: headers, query parameters and `HeadGuard` types. It has
//! no body guards yet, so a route that needs one is a bare route, which merges into the same
//! router.

use std::{net::SocketAddr, sync::Arc};

use bytes::Bytes;
use fast_http::{
    controller,
    core::{HeadGuard, Path, guard::header_has},
    format::{Body, Negotiated},
    router::{PatternError, Route, RouteList, Router, post},
    server::Server,
    transport::Tcp,
};
use http::{StatusCode, request::Parts};
use serde::{Deserialize, Serialize};

/// The state of the controller; handlers reach it through `&self`.
struct Library {
    name: &'static str,
}

/// Requests that carry the `x-staff` header.
struct Staff;

impl HeadGuard for Staff {
    type Output = ();

    fn check(parts: &Parts) -> Option<()> {
        header_has(parts, "x-staff").then_some(())
    }
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
    #[get(when(guard(Staff)))]
    async fn everything(&self) -> Negotiated<Listing> {
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

    #[get("/{id}", when(header.x_api_version = "2"))]
    async fn show_v2(&self, Path(id): Path<u64>) -> Negotiated<Item> {
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
}

/// The part of an order the body guard reads.
#[derive(Deserialize)]
struct Priority {
    express: bool,
}

async fn express(body: Body<Order>) -> (StatusCode, String) {
    (
        StatusCode::ACCEPTED,
        format!("express order for item {}", body.into_inner().item),
    )
}

async fn standard(body: Body<Order>) -> (StatusCode, String) {
    (
        StatusCode::ACCEPTED,
        format!("standard order for item {}", body.into_inner().item),
    )
}

fn is_express(_: &Parts, body: &Bytes) -> bool {
    serde_json::from_slice::<Priority>(body).is_ok_and(|priority| priority.express)
}

/// The router of the example.
pub fn app() -> Result<Router<impl RouteList>, PatternError> {
    let library = Arc::new(Library { name: "city" });

    Ok(Router::new().merge(library.routes()?).route(
        Route::new("/orders")?
            .endpoint(post(express).body_guard(is_express))
            .endpoint(post(standard)),
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
