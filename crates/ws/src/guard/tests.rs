use super::*;

fn parts(method: Method, connection: &str, upgrade: &str) -> Parts {
    http::Request::builder()
        .method(method)
        .header(CONNECTION, connection)
        .header(UPGRADE, upgrade)
        .body(())
        .expect("valid request")
        .into_parts()
        .0
}

#[test]
fn check_upgrade_request_matches() {
    assert!(
        WebSocketRequest::check(&parts(Method::GET, "keep-alive, Upgrade", "WebSocket")).is_some()
    );
}

#[test]
fn check_plain_get_misses() {
    let plain = http::Request::new(()).into_parts().0;

    assert!(WebSocketRequest::check(&plain).is_none());
}

#[test]
fn check_post_misses() {
    assert!(WebSocketRequest::check(&parts(Method::POST, "upgrade", "websocket")).is_none());
}
