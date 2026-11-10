use std::time::Duration;

use bytes::Bytes;
use fast_http_core::{Response as CoreResponse, Routes};
use fast_http_router::{Route, Router};
use fast_http_server::{Plain, Server, TaskWrap, transport::Memory};
use futures_util::{SinkExt, StreamExt};
use http::{Request, header};
use tokio::{sync::oneshot, task::JoinHandle};
use tokio_tungstenite::{
    client_async,
    tungstenite::{Message as Raw, client::IntoClientRequest},
};

use super::*;
use crate::{CloseFrame, Message, Utf8Bytes, close_code};

async fn echo(upgrade: WebSocketUpgrade) -> CoreResponse {
    upgrade
        .max_message_size(1024)
        .protocols(["chat.v2", "chat.v1"])
        .on_upgrade(|mut socket| async move {
            while let Some(Ok(message)) = socket.recv().await {
                match message {
                    Message::Close(_) => break,
                    other => {
                        if socket.send(other).await.is_err() {
                            break;
                        }
                    }
                }
            }
        })
}

async fn plain(upgrade: Option<WebSocketUpgrade>) -> &'static str {
    match upgrade {
        Some(_) => "upgradable",
        None => "plain",
    }
}

async fn limited(
    admission: Admission,
    upgrade: WebSocketUpgrade,
) -> Result<CoreResponse, AdmissionRejection> {
    Ok(upgrade
        .admit(&admission)?
        .on_upgrade(|mut socket| async move {
            let mut shutdown = socket.shutdown_signal();

            loop {
                tokio::select! {
                    message = socket.recv() => match message {
                        Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                        Some(Ok(other)) => {
                            if socket.send(other).await.is_err() {
                                break;
                            }
                        }
                    },
                    () = shutdown.wait() => {
                        let _ = socket.send(Message::Close(Some(CloseFrame {
                            code: close_code::AWAY,
                            reason: Utf8Bytes::from_static("shutdown"),
                        }))).await;

                        break;
                    }
                }
            }
        }))
}

tokio::task_local! {
    static TAG: &'static str;
}

#[derive(Clone)]
struct Tagged(&'static str);

impl TaskWrap for Tagged {
    fn wrap<F>(&self, task: F) -> impl Future<Output = F::Output> + Send + 'static
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        TAG.scope(self.0, task)
    }
}

async fn tagged(upgrade: WebSocketUpgrade) -> CoreResponse {
    upgrade.on_upgrade(|mut socket| async move {
        let tag = TAG.try_with(|tag| *tag).unwrap_or("none");

        let _ = socket.send(Message::text(tag)).await;
    })
}

fn routes(admission: Admission) -> impl Routes {
    Router::new()
        .route(Route::new("/ws").expect("pattern").get(echo))
        .route(Route::new("/either").expect("pattern").get(plain))
        .route(
            Route::new("/limited")
                .expect("pattern")
                .get(move |upgrade: WebSocketUpgrade| limited(admission.clone(), upgrade)),
        )
        .route(Route::new("/tagged").expect("pattern").get(tagged))
}

struct Running {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl Running {
    async fn stop(self) {
        let _ = self.stop.send(());

        tokio::time::timeout(Duration::from_secs(2), self.task)
            .await
            .expect("server stops in time")
            .expect("serve does not panic");
    }
}

async fn start() -> (fast_http_server::transport::MemoryConnector, Running) {
    start_with(Admission::unlimited(), Plain).await
}

async fn start_with<W: TaskWrap>(
    admission: Admission,
    wrap: W,
) -> (fast_http_server::transport::MemoryConnector, Running) {
    let (memory, connector) = Memory::new(64 * 1024);
    let server = Server::bind(&memory, ())
        .await
        .expect("bind")
        .task_wrap(wrap);
    let (stop, stopped) = oneshot::channel::<()>();
    let task = tokio::spawn(
        server
            .graceful_shutdown(async {
                let _ = stopped.await;
            })
            .serve(routes(admission)),
    );

    (connector, Running { stop, task })
}

fn handshake(path: &str, protocols: Option<&str>) -> Request<()> {
    let mut request = format!("ws://localhost{path}")
        .into_client_request()
        .expect("request");

    if let Some(protocols) = protocols {
        request.headers_mut().insert(
            header::SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_str(protocols).expect("header"),
        );
    }

    request
}

#[tokio::test]
async fn upgrade_echoes_text_and_selects_protocol() {
    let (connector, server) = start().await;
    let io = connector.connect().await.expect("connect");
    let (mut client, response) = client_async(handshake("/ws", Some("chat.v1, chat.v2")), io)
        .await
        .expect("handshake");

    client.send(Raw::text("ping me")).await.expect("send");

    let echoed = client.next().await.expect("message").expect("ok");

    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    assert_eq!(
        response.headers()[header::SEC_WEBSOCKET_PROTOCOL],
        "chat.v2"
    );
    assert_eq!(echoed, Raw::text("ping me"));

    server.stop().await;
}

#[tokio::test]
async fn upgrade_echoes_binary_frames() {
    let (connector, server) = start().await;
    let io = connector.connect().await.expect("connect");
    let (mut client, _) = client_async(handshake("/ws", None), io)
        .await
        .expect("handshake");

    client
        .send(Raw::Binary(Bytes::from_static(&[9, 8, 7])))
        .await
        .expect("send");

    let echoed = client.next().await.expect("message").expect("ok");

    assert_eq!(echoed, Raw::Binary(Bytes::from_static(&[9, 8, 7])));

    server.stop().await;
}

#[tokio::test]
async fn upgrade_oversized_message_closes_the_connection() {
    let (connector, server) = start().await;
    let io = connector.connect().await.expect("connect");
    let (mut client, _) = client_async(handshake("/ws", None), io)
        .await
        .expect("handshake");

    client
        .send(Raw::text("x".repeat(4096)))
        .await
        .expect("send");

    let closed = client.next().await;

    assert!(matches!(
        closed,
        None | Some(Err(_)) | Some(Ok(Raw::Close(_)))
    ));
    assert_eq!(close_code::SIZE, 1009);

    server.stop().await;
}

#[tokio::test]
async fn plain_get_to_upgrade_route_is_rejected() {
    let (connector, server) = start().await;
    let io = connector.connect().await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(io))
        .await
        .expect("h1");

    tokio::spawn(conn);

    let request = Request::builder()
        .uri("/ws")
        .body(http_body_util::Empty::<Bytes>::new())
        .expect("request");
    let response = sender.send_request(request).await.expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    server.stop().await;
}

#[tokio::test]
async fn optional_extractor_serves_plain_requests() {
    let (connector, server) = start().await;
    let io = connector.connect().await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(io))
        .await
        .expect("h1");

    tokio::spawn(conn);

    let request = Request::builder()
        .uri("/either")
        .body(http_body_util::Empty::<Bytes>::new())
        .expect("request");
    let response = sender.send_request(request).await.expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    server.stop().await;
}

#[tokio::test]
async fn on_upgrade_task_runs_through_the_task_wrap() {
    let (connector, server) = start_with(Admission::unlimited(), Tagged("wrapped")).await;
    let io = connector.connect().await.expect("connect");
    let (mut client, _) = client_async(handshake("/tagged", None), io)
        .await
        .expect("handshake");

    let message = client.next().await.expect("message").expect("ok");

    assert_eq!(message, Raw::text("wrapped"));

    server.stop().await;
}

#[tokio::test]
async fn on_upgrade_task_without_a_wrap_runs_unwrapped() {
    let (connector, server) = start().await;
    let io = connector.connect().await.expect("connect");
    let (mut client, _) = client_async(handshake("/tagged", None), io)
        .await
        .expect("handshake");

    let message = client.next().await.expect("message").expect("ok");

    assert_eq!(message, Raw::text("none"));

    server.stop().await;
}

async fn status_of(
    connector: &fast_http_server::transport::MemoryConnector,
    path: &str,
) -> StatusCode {
    let io = connector.connect().await.expect("connect");

    match client_async(handshake(path, None), io).await {
        Ok((_, response)) => response.status(),
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => response.status(),
        Err(other) => panic!("unexpected handshake error: {other}"),
    }
}

#[tokio::test]
async fn admit_over_the_limit_is_rejected_with_503() {
    let admission = Admission::new(1);
    let (connector, server) = start_with(admission.clone(), Plain).await;
    let io = connector.connect().await.expect("connect");
    let (first, _) = client_async(handshake("/limited", None), io)
        .await
        .expect("first admitted");

    assert_eq!(admission.active(), 1);
    assert_eq!(
        status_of(&connector, "/limited").await,
        StatusCode::SERVICE_UNAVAILABLE
    );

    drop(first);
    server.stop().await;
}

#[tokio::test]
async fn admit_frees_the_slot_when_the_socket_closes() {
    let admission = Admission::new(1);
    let (connector, server) = start_with(admission.clone(), Plain).await;
    let io = connector.connect().await.expect("connect");
    let (mut first, _) = client_async(handshake("/limited", None), io)
        .await
        .expect("first admitted");

    first.close(None).await.expect("close");

    tokio::time::timeout(Duration::from_secs(2), admission.drained())
        .await
        .expect("slot is freed");

    assert_eq!(
        status_of(&connector, "/limited").await,
        StatusCode::SWITCHING_PROTOCOLS
    );

    server.stop().await;
}

#[tokio::test]
async fn shutdown_tells_live_sockets_to_close_and_drains() {
    let admission = Admission::new(4);
    let (connector, server) = start_with(admission.clone(), Plain).await;
    let io = connector.connect().await.expect("connect");
    let (mut client, _) = client_async(handshake("/limited", None), io)
        .await
        .expect("admitted");

    client.send(Raw::text("sync")).await.expect("send");
    client.next().await.expect("echo").expect("ok");

    admission.shutdown();

    let closing = client.next().await.expect("close frame").expect("ok");

    assert!(
        matches!(closing, Raw::Close(Some(frame)) if u16::from(frame.code) == close_code::AWAY)
    );

    drop(client);
    tokio::time::timeout(Duration::from_secs(2), admission.drained())
        .await
        .expect("drains");

    assert_eq!(
        status_of(&connector, "/limited").await,
        StatusCode::SERVICE_UNAVAILABLE
    );

    server.stop().await;
}
