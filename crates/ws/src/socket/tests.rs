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

#[tokio::test]
async fn close_reports_a_connection_dropped_before_the_reply() {
    let (server, mut client) = pair().await;
    let closing = tokio::spawn(server.close());

    let frame = client.next().await.expect("close frame").expect("ok");

    assert!(matches!(frame, tungstenite::Message::Close(_)));

    drop(client);

    let result = tokio::time::timeout(Duration::from_secs(1), closing)
        .await
        .expect("close finishes once the connection drops")
        .expect("close task does not panic");

    assert!(result.is_err());
}

#[tokio::test]
async fn close_after_the_peer_reset_reports_the_reset() {
    let (mut server, client) = pair().await;

    drop(client);

    let read = server.recv().await.expect("the reset is reported");

    assert!(read.is_err());

    let result = server.close().await;

    assert!(
        matches!(
            result.map_err(Error::into_inner),
            Err(tungstenite::Error::Protocol(
                ProtocolError::ResetWithoutClosingHandshake
            ))
        ),
        "a reset connection did not complete a close handshake"
    );
}

#[tokio::test(start_paused = true)]
async fn close_flushes_the_frame_of_a_cancelled_sink_close() {
    let (server, client) = duplex(16);
    let mut server = WebSocket::from_io(server, None, WebSocketConfig::default()).await;
    let mut client = WebSocketStream::from_raw_socket(client, Role::Client, None).await;
    let pause = Duration::from_millis(50);

    let blocked =
        tokio::time::timeout(pause, server.send(Message::Binary(vec![0; 64].into()))).await;

    assert!(blocked.is_err(), "the pipe is full, so the send stalls");

    let cancelled = tokio::time::timeout(pause, SinkExt::close(&mut server)).await;

    assert!(cancelled.is_err(), "the pipe is full, so the close stalls");

    let closing = tokio::spawn(server.close());
    let data = client.next().await.expect("data").expect("ok");
    let frame = tokio::time::timeout(Duration::from_secs(1), client.next())
        .await
        .expect("the queued close frame is flushed")
        .expect("close frame")
        .expect("ok");

    assert!(matches!(data, tungstenite::Message::Binary(_)));
    assert!(matches!(frame, tungstenite::Message::Close(_)));
    assert!(client.next().await.is_none());

    let result = closing.await.expect("close task does not panic");

    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn close_after_a_protocol_error_reports_that_error() {
    let (server, mut client) = duplex(4096);
    let mut server = WebSocket::from_io(server, None, WebSocketConfig::default()).await;
    let reserved_bit_set = [0xC1, 0x80, 0, 0, 0, 0];

    tokio::io::AsyncWriteExt::write_all(&mut client, &reserved_bit_set)
        .await
        .expect("raw frame is written");

    let read = server.recv().await.expect("the protocol error is reported");

    assert!(read.is_err());

    let result = server.close().await.map_err(Error::into_inner);

    assert!(
        matches!(
            result,
            Err(tungstenite::Error::Protocol(
                ProtocolError::NonZeroReservedBits
            ))
        ),
        "{result:?}"
    );
}

#[test]
fn duplicate_keeps_the_kind_and_message_of_an_io_error() {
    let original = tungstenite::Error::Io(io::Error::new(io::ErrorKind::BrokenPipe, "gone"));

    let copy = duplicate(&original);

    assert!(
        matches!(&copy, tungstenite::Error::Io(error) if error.kind() == io::ErrorKind::BrokenPipe && error.to_string() == "gone"),
        "{copy:?}"
    );
}
