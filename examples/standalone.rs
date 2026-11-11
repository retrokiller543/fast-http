//! Routes without a controller: lone functions and a state-free module, with the same typed
//! guards. Run it with `cargo run --example standalone --features macros`, then:
//!
//! ```sh
//! curl http://127.0.0.1:3000/health
//! curl http://127.0.0.1:3000/admin
//! curl -H 'x-admin: 1' http://127.0.0.1:3000/admin
//! curl http://127.0.0.1:3000/items
//! curl -H 'x-all: 1' http://127.0.0.1:3000/items
//! curl http://127.0.0.1:3000/items/0
//! curl http://127.0.0.1:3000/items/7
//! curl -H 'content-type: application/json' -d '{"item":7,"express":true}' http://127.0.0.1:3000/items/orders
//! ```
//!
//! A lone `#[get(..)]` function becomes an endpoint named after the function, and `name.route()`
//! builds its route. It is a complete endpoint: when its guard misses, the request is answered
//! with the guard miss, not handed to another function. To give one path several guarded
//! variants, put the functions in a `#[routes]` module: they are grouped, extracted lazily and at
//! most once, exactly like the methods of a `#[controller]`. Routes are ordered most specific first, so `/items/orders` wins
//! over `/items/{id}` whatever order they are declared in.
//!
//! The one rule for bodies: variants of one method and path that read the body must all take the
//! same body type. If a body can take several shapes, make it one enum and guard on its variants.

use std::net::SocketAddr;

use fast_http::{
    get,
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
    async fn express(Body { value: order, .. }: Body<Order>) -> (StatusCode, String) {
        (
            StatusCode::ACCEPTED,
            format!("express order for item {}", order.item),
        )
    }

    #[post("/orders")]
    async fn standard(order: Body<Order>) -> (StatusCode, String) {
        (
            StatusCode::ACCEPTED,
            format!("standard order for item {}", order.value.item),
        )
    }
}

/// The router of the example.
pub fn app() -> Result<Router<impl RouteList>, PatternError> {
    Ok(Router::new()
        .route(health.route()?)
        .route(admin.route()?)
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
