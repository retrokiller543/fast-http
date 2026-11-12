#![cfg(feature = "macros")]

use std::sync::Arc;

use bytes::Bytes;
use fast_http::{
    controller,
    core::{Path, Routes},
    get,
    router::{Controller, Router},
};
use http::{Method, Request, StatusCode};
use http_body_util::{BodyExt, Full};

#[controller("/library", groups(catalog, admin))]
struct Library {
    name: &'static str,
}

#[controller(group = catalog)]
impl Library {
    #[get]
    async fn index(&self) -> String {
        format!("{} index", self.name)
    }

    #[get("/items/{id}")]
    async fn item(&self, Path(id): Path<u64>) -> String {
        format!("item {id}")
    }
}

mod admin {
    use fast_http::controller;
    use http::StatusCode;

    use super::Library;

    #[controller(group = admin, "/admin")]
    impl Library {
        #[get("/stats", when(headers.contains_key("x-staff")))]
        async fn stats(&self) -> &'static str {
            "stats"
        }

        #[delete("/items/{id}")]
        async fn remove(&self) -> StatusCode {
            StatusCode::NO_CONTENT
        }
    }
}

#[controller("/shop", groups(products, orders))]
struct Shop;

#[controller(group = products)]
impl Shop {
    #[get("/items/{id}")]
    async fn product(&self, Path(id): Path<u64>) -> String {
        format!("product {id}")
    }
}

#[controller(group = orders)]
impl Shop {
    #[get("/items/orders")]
    async fn orders(&self) -> &'static str {
        "orders"
    }
}

#[get("/health")]
async fn health() -> &'static str {
    "ok"
}

async fn send<R: Routes>(
    router: &R,
    method: Method,
    uri: &str,
    staff: bool,
) -> (StatusCode, String) {
    let mut builder = Request::builder().method(method).uri(uri);

    if staff {
        builder = builder.header("x-staff", "1");
    }

    let request = builder
        .body(Full::new(Bytes::new()))
        .expect("valid request");
    let response = router.dispatch(request).await;
    let status = response.status();
    let text = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    (status, String::from_utf8(text.to_vec()).expect("utf-8"))
}

fn library() -> Router<impl fast_http::router::RouteList> {
    Arc::new(Library { name: "central" })
        .routes()
        .expect("valid routes")
}

#[tokio::test]
async fn groups_share_the_controller_path_and_state() {
    let router = library();

    assert_eq!(
        send(&router, Method::GET, "/library", false).await.1,
        "central index"
    );
    assert_eq!(
        send(&router, Method::GET, "/library/items/7", false)
            .await
            .1,
        "item 7"
    );
}

#[tokio::test]
async fn group_in_another_module_adds_its_own_prefix() {
    let router = library();

    let stats = send(&router, Method::GET, "/library/admin/stats", true).await;
    let removed = send(&router, Method::DELETE, "/library/admin/items/7", false).await;
    let refused = send(&router, Method::GET, "/library/admin/stats", false).await;

    assert_eq!(stats.1, "stats");
    assert_eq!(removed.0, StatusCode::NO_CONTENT);
    assert_eq!(refused.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn more_specific_route_of_a_later_group_still_wins() {
    let router = Arc::new(Shop).routes().expect("valid routes");

    assert_eq!(
        send(&router, Method::GET, "/shop/items/orders", false)
            .await
            .1,
        "orders"
    );
    assert_eq!(
        send(&router, Method::GET, "/shop/items/3", false).await.1,
        "product 3"
    );
}

#[tokio::test]
async fn controller_merges_with_lone_functions() {
    let router = Router::new().route(health).merge(
        Arc::new(Library { name: "branch" })
            .routes()
            .expect("valid routes"),
    );

    assert_eq!(send(&router, Method::GET, "/health", false).await.1, "ok");
    assert_eq!(
        send(&router, Method::GET, "/library", false).await.1,
        "branch index"
    );
}

#[test]
fn controller_path_is_the_struct_attribute_path() {
    assert_eq!(Library::PATH, "/library");
    assert_eq!(Shop::PATH, "/shop");
}
