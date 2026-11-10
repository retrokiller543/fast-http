use tokio::io::duplex;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use super::*;
use crate::message::close_code;

async fn pair() -> (
    WebSocket<tokio::io::DuplexStream>,
    WebSocketStream<tokio::io::DuplexStream>,
) {
    let (server, client) = duplex(4096);

    (
        WebSocket::from_io(server, None, WebSocketConfig::default()).await,
        WebSocketStream::from_raw_socket(client, Role::Client, None).await,
    )
}

#[tokio::test]
async fn recv_text_from_client() {
    let (mut server, mut client) = pair().await;

    client
        .send(tungstenite::Message::text("hello"))
        .await
        .expect("client send");

    let message = server.recv().await.expect("message").expect("ok");

    assert_eq!(message, Message::text("hello"));
}

#[tokio::test]
async fn send_binary_reaches_client() {
    let (mut server, mut client) = pair().await;

    server
        .send(Message::binary(vec![1, 2, 3]))
        .await
        .expect("server send");

    let message = client.next().await.expect("message").expect("ok");

    assert_eq!(message.into_data().as_ref(), &[1, 2, 3]);
}

#[tokio::test]
async fn recv_after_client_close_yields_close_then_none() {
    let (mut server, mut client) = pair().await;

    client.close(None).await.expect("client close");

    let first = server.recv().await.expect("message").expect("ok");

    assert!(matches!(first, Message::Close(_)));
    assert!(server.recv().await.is_none());
}

#[tokio::test]
async fn oversized_message_is_rejected() {
    let (server, client) = duplex(4096);
    let config = WebSocketConfig::default().max_message_size(Some(4));
    let mut server = WebSocket::from_io(server, None, config).await;
    let mut client = WebSocketStream::from_raw_socket(client, Role::Client, None).await;

    client
        .send(tungstenite::Message::text("too long for four"))
        .await
        .expect("client send");

    let result = server.recv().await.expect("message");

    assert!(result.is_err());
    assert_eq!(close_code::SIZE, 1009);
}

#[tokio::test]
async fn split_halves_work_independently() {
    let (server, mut client) = pair().await;
    let (mut sender, mut receiver) = server.split();

    client
        .send(tungstenite::Message::text("in"))
        .await
        .expect("client send");
    sender.send(Message::text("out")).await.expect("send half");

    let incoming = receiver.next().await.expect("message").expect("ok");
    let outgoing = client.next().await.expect("message").expect("ok");

    assert_eq!(incoming, Message::text("in"));
    assert_eq!(outgoing, tungstenite::Message::text("out"));
}
