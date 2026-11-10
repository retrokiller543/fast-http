# fast-http-server

## Role

Runs `fast_http_core::Routes` on any `fast_http_transport::Transport`. It provides the accept loop, a swappable protocol `Engine`, connection and request limits, and graceful shutdown.

## Usage

```rust
let routes = Router::new().route(Route::new("/users/{id}")?.get(show));
let server = Server::bind(&Tcp::new(), "127.0.0.1:3000".parse()?)
    .await?
    .config::<MyPreset>()
    .settings(|s| s.drain_timeout = Duration::from_secs(5));

server.serve(routes).await;

// Optionally stop on a signal instead of running until the process ends:
// server.graceful_shutdown(async { tokio::signal::ctrl_c().await.ok(); }).serve(routes).await;
```

- Request bodies are capped at 8 MiB by default (`DEFAULT_BODY_LIMIT`); raise `max_body_bytes` for larger uploads.
- Limits come from a `ServerConfig` preset (associated consts, `DefaultConfig` by default). The server copies them into `Settings`, which you can override at runtime.
- The server owns the routes, shared between connection tasks through one `Arc`.
- Plain TCP is the expected setup behind a TLS-terminating proxy. For direct exposure, wrap the transport in `Tls` (the `tls` feature).
- Graceful shutdown is optional (`.graceful_shutdown(signal)`). On shutdown the server stops accepting, the engine finishes in-flight requests (h2 GOAWAY, h1 `Connection: close`), and connections still open after `drain_timeout` are aborted.
- `.task_wrap(w)` wraps every task the server spawns (connection tasks, HTTP/2 stream tasks and detached tasks handlers spawn through `fast_http_core::TaskSpawner`, such as upgraded WebSockets) with a `TaskWrap`, so an embedding framework can carry its context, such as Upwell's DI context or a tracing span, into handlers. The default `Plain` spawns tasks unchanged.
- `just serve` runs `examples/hello.rs`: plain HTTP/1.1 and h2c on 127.0.0.1:3000.

## Internal role

Each accepted connection is spawned onto a `JoinSet`. The establish step (the TLS handshake) runs there, bounded by `handshake_timeout`. While `max_connections` connections are open, the loop stops accepting. The default `Hyper` engine serves HTTP/1.1 and HTTP/2 on every connection: cleartext h2 is detected by its prior-knowledge preface, and over TLS the client's ALPN choice decides. The engine is a marker type whose per-server `State<W>` is built once for the server's `TaskWrap`; `Hyper` puts the wrapper into its hyper executor (`Spawner`), so h2 stream tasks are wrapped too. Request bodies are wrapped in `Limited` (`max_body_bytes`), and `PeerAddr` and the server's `TaskSpawner` are carried around each poll of the request, not stored in its extensions, so neither allocates. Logs use the `fast_http::server` target.

## Feature flags

- `tls`: forwards to `fast-http-transport/tls`.
