# fast-http-transport

## Role

Byte transports. A `Transport` binds an address to a `Listener`, which yields `Accepted` connections. Establishing one (for example, running its TLS handshake) produces a `Connection`: a readiness-based `AsyncRead + AsyncWrite` stream plus `ConnInfo` (peer address, ALPN protocol, whether it's encrypted). Nothing here knows about HTTP.

## Usage

```rust
let plain = Tcp::new();
let tls = Tls::new(Tcp::new(), Arc::new(server_config_from_files("cert.pem", "key.pem")?));
let (memory, connector) = Memory::new(64 * 1024);
```

- `Tcp`: tokio TCP, with `TCP_NODELAY` on by default.
- `Tls<T>`: rustls over any inner transport (requires the `tls` feature). `server_config` uses the ring provider, safe default protocol versions, and ALPN offering `h2` then `http/1.1`.
- `Memory`: `tokio::io::duplex` pipes for socket-free tests. `MemoryConnector::connect` returns the client end.

## Internal role

Accepting and establishing are separate steps, so the server runs handshakes on each connection's own task and a slow client can't stall the accept loop. `Listener::accept` must be cancel-safe. Only the readiness I/O model exists today. A completion-based (io_uring) transport would add an I/O capability next to `Connection` (see `HTTP-DESIGN.md` §3.2).

## Feature flags

- `tls`: enables `Tls`, `server_config` and `server_config_from_files` through `rustls` and `tokio-rustls`, using the ring provider only (no aws-lc, no openssl).
