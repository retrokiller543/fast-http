use super::*;
use crate::testing::{Tag, header, router, send};

#[tokio::test]
async fn handle_path_under_prefix_runs_flow() {
    let router = router().layer(Scope::new("/trace", Tag("a")));

    let response = send(&router, "/trace").await;

    assert_eq!(header(&response, "x-trace-out"), "a");
}

#[tokio::test]
async fn handle_path_outside_prefix_skips_flow() {
    let router = router().layer(Scope::new("/other", Tag("a")));

    let response = send(&router, "/trace").await;

    assert_eq!(header(&response, "x-trace-out"), "-");
}

#[tokio::test]
async fn handle_prefix_sharing_only_a_string_prefix_skips_flow() {
    let router = router().layer(Scope::new("/tra", Tag("a")));

    let response = send(&router, "/trace").await;

    assert_eq!(header(&response, "x-trace-out"), "-");
}

#[tokio::test]
async fn handle_empty_prefix_covers_every_path() {
    let router = router().layer(Scope::new("", Tag("a")));

    let response = send(&router, "/trace").await;

    assert_eq!(header(&response, "x-trace-out"), "a");
}
