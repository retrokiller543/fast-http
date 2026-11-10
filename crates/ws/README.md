# fast-http-ws

## Role

WebSocket upgrades for fast-http over hyper. The types are shaped after `axum::extract::ws` (`WebSocketUpgrade`, `WebSocket`, `Message`, `CloseFrame`, `close_code`), so a protocol written against one can be driven by the other with only the import changed.

## Usage

```rust
async fn echo(upgrade: WebSocketUpgrade) -> Response {
    upgrade
        .max_message_size(64 * 1024)
        .protocols(["chat.v2", "chat.v1"])
        .on_upgrade(|mut socket| async move {
            while let Some(Ok(message)) = socket.recv().await {
                if matches!(message, Message::Close(_)) || socket.send(message).await.is_err() {
                    break;
                }
            }
        })
}

let routes = Router::new().route(Route::new("/ws")?.get(echo));
```

- `WebSocketUpgrade` is a head extractor. It validates the RFC 6455 handshake and rejects anything else with a typed `WebSocketUpgradeRejection` (400, 405, 426 or 500). `Option<WebSocketUpgrade>` serves a plain response on the same path.
- `protocols` takes the server's subprotocols in preference order and picks the first the client offered; `selected_protocol()` shows the choice before `on_upgrade`.
- `WebSocket` is a `Stream` of `Message` and a `Sink<Message>`, plus `recv`, `send`, `close` and `protocol`. `StreamExt::split` gives independent halves.
- `WebSocketRequest` is a `HeadGuard` matching upgrade requests, for use with `Branch` or endpoint guards.
- Limits: `max_message_size`, `max_frame_size` and the buffer sizes.
- Admission control: an `Admission` caps the live sockets of an endpoint and counts them. `upgrade.admit(&admission)?` takes a slot before anything is upgraded and fails with `AdmissionRejection`, which renders as `503` with `Retry-After`. The slot is held until the `WebSocket` is dropped, so `admission.active()` is the number of open sockets.
- Shutdown: `admission.shutdown()` stops admitting and fires every live socket's `socket.shutdown_signal()` (`signal.wait()` sits next to `recv` in a `select!`); `admission.drained().await` resolves once the last socket is gone. Wire these to the server's shutdown signal: upgraded sockets are detached tasks, so the server's own drain does not wait for them.

```rust
let admission = Admission::new(1_000);
let router = Router::new().route(
    Route::new("/ws")?.get(move |upgrade: WebSocketUpgrade| ws(admission.clone(), upgrade)),
);

async fn ws(admission: Admission, upgrade: WebSocketUpgrade) -> Result<Response, AdmissionRejection> {
    Ok(upgrade.admit(&admission)?.on_upgrade(|mut socket| async move {
        let mut shutdown = socket.shutdown_signal();

        loop {
            tokio::select! {
                message = socket.recv() => { /* handle, break on close */ }
                () = shutdown.wait() => break,
            }
        }
    }))
}
```

State such as the `Admission` is captured by the handler closure; fast-http has no state registry.

## Internal role

The handshake answer is built here (`Sec-WebSocket-Accept` from `tungstenite`, `101`, optional `Sec-WebSocket-Protocol`). After hyper sends it, a spawned task awaits the `OnUpgrade` handle the engine left in the request extensions and wraps the upgraded connection in a `tokio-tungstenite` `WebSocketStream`. The server engine already serves with upgrades enabled.

The task is spawned through the `TaskSpawner` the server engine carries around each request, so the server's `TaskWrap` applies to it and it runs in the context of the request that upgraded. Without a server (a router driven directly) it falls back to `tokio::spawn`.

Only HTTP/1.1 upgrades are supported. WebSockets over HTTP/2 (RFC 8441 extended CONNECT) are rejected with `NotUpgradable`.

## Feature flags

None.
