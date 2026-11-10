use std::{net::SocketAddr, sync::Arc, time::Duration};

use fast_http_transport::{Alpn, Memory, Tcp, Tls, server_config};
use http::{Method, StatusCode, Version};
use tokio::{
    io::AsyncReadExt,
    net::TcpStream,
    time::{sleep, timeout},
};

use super::*;
use crate::testing::{
    CERT, KEY, STARTED, get_request, h1, h2, request, spawn, start, text, tls_connect,
};

#[tokio::test]
async fn serves_without_a_shutdown_signal() {
    let (memory, connector) = Memory::new(64 * 1024);
    let server = Server::bind(&memory, ()).await.expect("bind");
    let task = tokio::spawn(server.serve(crate::testing::routes()));
    let mut client = h1(connector.connect().await.expect("connect")).await;
    let response = client
        .send_request(get_request("/hello"))
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    task.abort();
}

#[tokio::test]
async fn http1_request_reaches_handler() {
    let (connector, server) = start(|_| {}).await;
    let mut client = h1(connector.connect().await.expect("connect")).await;
    let response = client
        .send_request(get_request("/hello"))
        .await
        .expect("response");

    assert_eq!(response.version(), Version::HTTP_11);
    assert_eq!(text(response).await, "hello");

    server.stop().await;
}

#[tokio::test]
async fn cleartext_h2_is_detected_by_preface() {
    let (connector, server) = start(|_| {}).await;
    let mut client = h2(connector.connect().await.expect("connect")).await;
    let response = client
        .send_request(get_request("/hello"))
        .await
        .expect("response");

    assert_eq!(response.version(), Version::HTTP_2);
    assert_eq!(text(response).await, "hello");

    server.stop().await;
}

#[tokio::test]
async fn unknown_path_renders_not_found() {
    let (connector, server) = start(|_| {}).await;
    let mut client = h1(connector.connect().await.expect("connect")).await;
    let response = client
        .send_request(get_request("/nope"))
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

async fn peer_over_tcp(peer_addr: bool) -> (String, SocketAddr) {
    let server = Server::bind(&Tcp::new(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind")
        .settings(|settings| settings.peer_addr = peer_addr);
    let addr = server.local_addr().expect("local addr");
    let running = spawn(server);
    let stream = TcpStream::connect(addr).await.expect("connect");
    let client_addr = stream.local_addr().expect("client addr");
    let mut client = h1(stream).await;
    let response = client
        .send_request(get_request("/peer"))
        .await
        .expect("response");
    let body = text(response).await;

    running.stop().await;

    (body, client_addr)
}

#[tokio::test]
async fn peer_addr_is_inserted_over_tcp() {
    let (body, client_addr) = peer_over_tcp(true).await;

    assert_eq!(body, client_addr.to_string());
}

#[tokio::test]
async fn peer_addr_can_be_disabled() {
    let (body, _) = peer_over_tcp(false).await;

    assert_eq!(body, "none");
}

#[tokio::test]
async fn body_within_server_limit_is_read() {
    let (connector, server) = start(|settings| settings.max_body_bytes = 8).await;
    let mut client = h1(connector.connect().await.expect("connect")).await;
    let response = client
        .send_request(request(Method::POST, "/echo", b"12345678"))
        .await
        .expect("response");

    assert_eq!(text(response).await, "12345678");

    server.stop().await;
}

#[tokio::test]
async fn body_over_server_limit_is_rejected() {
    let (connector, server) = start(|settings| settings.max_body_bytes = 8).await;
    let mut client = h1(connector.connect().await.expect("connect")).await;
    let response = client
        .send_request(request(Method::POST, "/echo", b"123456789"))
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    server.stop().await;
}

#[tokio::test]
async fn shutdown_finishes_in_flight_request() {
    let (connector, server) = start(|_| {}).await;
    let mut client = h1(connector.connect().await.expect("connect")).await;
    let in_flight = tokio::spawn(async move { client.send_request(get_request("/slow")).await });

    STARTED.notified().await;
    server.stop().await;

    let response = in_flight.await.expect("task").expect("response");

    assert_eq!(text(response).await, "slow");
}

#[tokio::test]
async fn drain_timeout_aborts_stuck_requests() {
    let (connector, server) =
        start(|settings| settings.drain_timeout = Duration::from_millis(50)).await;
    let mut client = h1(connector.connect().await.expect("connect")).await;
    let in_flight = tokio::spawn(async move { client.send_request(get_request("/stuck")).await });

    STARTED.notified().await;
    server.stop().await;

    let result = timeout(Duration::from_secs(1), in_flight)
        .await
        .expect("client sees the abort")
        .expect("task");

    assert!(result.is_err());
}

#[tokio::test]
async fn shutdown_stops_accepting() {
    let (connector, server) = start(|_| {}).await;

    server.stop().await;

    let error = connector.connect().await.expect_err("listener is gone");

    assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
}

#[tokio::test]
async fn connection_limit_queues_extra_connections() {
    let (connector, server) = start(|settings| settings.max_connections = 1).await;
    let mut first = h1(connector.connect().await.expect("connect")).await;
    let mut second = h1(connector.connect().await.expect("connect")).await;

    first
        .send_request(get_request("/hello"))
        .await
        .expect("first response");

    let queued = tokio::spawn(async move { second.send_request(get_request("/hello")).await });

    sleep(Duration::from_millis(50)).await;
    assert!(!queued.is_finished());

    drop(first);

    let response = timeout(Duration::from_secs(1), queued)
        .await
        .expect("queued connection is accepted")
        .expect("task")
        .expect("response");

    assert_eq!(text(response).await, "hello");

    server.stop().await;
}

async fn tls_version(alpn: Alpn) -> Version {
    let (memory, connector) = Memory::new(64 * 1024);
    let config = Arc::new(server_config(CERT, KEY).expect("fixture config"));
    let server = Server::bind(&Tls::new(memory, config), ())
        .await
        .expect("bind");
    let running = spawn(server);
    let tls = tls_connect(connector.connect().await.expect("connect"), alpn).await;
    let response = match alpn {
        Alpn::H2 => h2(tls).await.send_request(get_request("/hello")).await,
        _ => h1(tls).await.send_request(get_request("/hello")).await,
    }
    .expect("response");
    let version = response.version();

    assert_eq!(text(response).await, "hello");

    running.stop().await;

    version
}

#[tokio::test]
async fn tls_serves_h2_when_negotiated() {
    assert_eq!(tls_version(Alpn::H2).await, Version::HTTP_2);
}

#[tokio::test]
async fn tls_serves_http1_when_negotiated() {
    assert_eq!(tls_version(Alpn::Http11).await, Version::HTTP_11);
}

#[tokio::test]
async fn silent_tls_client_is_closed_after_handshake_timeout() {
    let (memory, connector) = Memory::new(64 * 1024);
    let config = Arc::new(server_config(CERT, KEY).expect("fixture config"));
    let server = Server::bind(&Tls::new(memory, config), ())
        .await
        .expect("bind")
        .settings(|settings| settings.handshake_timeout = Duration::from_millis(50));
    let running = spawn(server);
    let mut raw = connector.connect().await.expect("connect");
    let mut buf = [0; 1];

    let read = timeout(Duration::from_secs(1), raw.read(&mut buf))
        .await
        .expect("server closes the connection")
        .expect("read");

    assert_eq!(read, 0);

    running.stop().await;
}
