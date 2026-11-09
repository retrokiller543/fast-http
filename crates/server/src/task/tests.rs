use fast_http_transport::{Memory, MemoryConnector};

use crate::{
    Server,
    testing::{Running, Tagged, get_request, h1, h2, spawn, text},
};

async fn start_tagged() -> (MemoryConnector, Running) {
    let (memory, connector) = Memory::new(64 * 1024);
    let server = Server::bind(&memory, ())
        .await
        .expect("bind")
        .task_wrap(Tagged("wrapped"));

    (connector, spawn(server))
}

#[tokio::test]
async fn http1_handler_runs_inside_wrapped_connection_task() {
    let (connector, server) = start_tagged().await;
    let mut client = h1(connector.connect().await.expect("connect")).await;
    let response = client
        .send_request(get_request("/tag"))
        .await
        .expect("response");

    assert_eq!(text(response).await, "wrapped");

    server.stop().await;
}

#[tokio::test]
async fn http2_handler_runs_inside_wrapped_stream_task() {
    let (connector, server) = start_tagged().await;
    let mut client = h2(connector.connect().await.expect("connect")).await;
    let response = client
        .send_request(get_request("/tag"))
        .await
        .expect("response");

    assert_eq!(text(response).await, "wrapped");

    server.stop().await;
}

#[tokio::test]
async fn plain_server_leaves_tasks_unwrapped() {
    let (connector, server) = crate::testing::start(|_| {}).await;
    let mut client = h2(connector.connect().await.expect("connect")).await;
    let response = client
        .send_request(get_request("/tag"))
        .await
        .expect("response");

    assert_eq!(text(response).await, "none");

    server.stop().await;
}
