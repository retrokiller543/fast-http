# fast-http-server

## Role

Runs `fast_http_core::Routes` on any `fast_http_transport::Transport`. It provides the accept loop, a swappable protocol `Engine`, connection and request limits, and graceful shutdown.

## Usage

```rust
let routes = Router::new((Route::new("/users/{id}", (get(show),))?,));
let server = Server::bind(&Tcp::new(), "127.0.0.1:3000".parse()?)
    .await?
    .config::<MyPreset>()
    .settings(|s| s.drain_timeout = Duration::from_secs(5));

server.serve(routes, async { tokio::signal::ctrl_c().await.ok(); }).await;
```

- Limits come from a `ServerConfig` preset (associated consts, `DefaultConfig` by default). The server copies them into `Settings`, which you can override at runtime.
- The server owns the routes, shared between connection tasks through one `Arc`.
- Plain TCP is the expected setup behind a TLS-terminating proxy. For direct exposure, wrap the transport in `Tls` (the `tls` feature).
- On shutdown the server stops accepting, the engine finishes in-flight requests (h2 GOAWAY, h1 `Connection: close`), and connections still open after `drain_timeout` are aborted.
- `just serve` runs `examples/hello.rs`: plain HTTP/1.1 and h2c on 127.0.0.1:3000.

## Internal role

Each accepted connection is spawned onto a `JoinSet`. The establish step (the TLS handshake) runs there, bounded by `handshake_timeout`. While `max_connections` connections are open, the loop stops accepting. The default `Hyper` engine serves HTTP/1.1 and HTTP/2 on every connection: cleartext h2 is detected by its prior-knowledge preface, and over TLS the client's ALPN choice decides. Request bodies are wrapped in `Limited` (`max_body_bytes`), and a `PeerAddr` extension is inserted unless `peer_addr` is off. Logs use the `fast_http::server` target.

## Feature flags

- `tls`: forwards to `fast-http-transport/tls`.
