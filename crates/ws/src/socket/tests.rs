use tokio::io::duplex;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use super::*;
use crate::{Utf8Bytes, message::close_code};

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

#[tokio::test]
async fn close_with_sends_the_frame_and_waits_for_the_reply() {
    let (server, mut client) = pair().await;
    let closing = tokio::spawn(server.close_with(CloseFrame {
        code: close_code::AWAY,
        reason: Utf8Bytes::from_static("bye"),
    }));

    let frame = client.next().await.expect("close frame").expect("ok");

    assert!(
        matches!(frame, tungstenite::Message::Close(Some(frame)) if u16::from(frame.code) == close_code::AWAY)
    );
    assert!(client.next().await.is_none());

    tokio::time::timeout(Duration::from_secs(1), closing)
        .await
        .expect("close completes once the client answers")
        .expect("close task does not panic")
        .expect("close succeeds");
}

#[tokio::test]
async fn close_after_the_peer_closed_still_succeeds() {
    let (mut server, mut client) = pair().await;

    client.close(None).await.expect("client close");

    let first = server.recv().await.expect("message").expect("ok");

    assert!(matches!(first, Message::Close(_)));
    let result = server.close().await;

    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test(start_paused = true)]
async fn close_gives_up_when_the_peer_never_reads() {
    let (server, _client) = duplex(16);
    let server = WebSocket::from_io(server, None, WebSocketConfig::default()).await;
    let frame = CloseFrame {
        code: close_code::AWAY,
        reason: Utf8Bytes::from_static("a reason longer than the sixteen byte pipe"),
    };

    let result = tokio::time::timeout(CLOSE_TIMEOUT * 2, server.close_with(frame)).await;

    assert!(matches!(result, Ok(Ok(()))));
}
