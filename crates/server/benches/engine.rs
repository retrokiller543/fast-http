//! Allocations and time per HTTP/1.1 request through the hyper engine, over an in-memory
//! connection, so engine costs show without a network.
//!
//! The server and the raw-bytes client share one current-thread runtime and the process-wide
//! allocation counter, so nothing else allocates while a request is measured. The client reuses
//! one buffer and one connection, which keeps connection setup out of every count.

use criterion::{Criterion, criterion_group, criterion_main};
use fast_http_bench_support::{Budget, Measurement, bench, criterion, measure};
use fast_http_core::Path;
use fast_http_format::Negotiated;
use fast_http_router::{Route, Router};
use fast_http_server::Server;
use fast_http_transport::Memory;
use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, DuplexStream},
    runtime::{Builder, Runtime},
};

const CASES: [(&str, &[u8]); 2] = [
    ("plaintext", b"GET /health HTTP/1.1\r\nhost: bench\r\n\r\n"),
    ("json", b"GET /users/42 HTTP/1.1\r\nhost: bench\r\n\r\n"),
];

/// Requests averaged for the per-request line.
const REQUESTS: u64 = 1000;

/// JSON body of the `json` case.
#[derive(Serialize)]
struct User {
    id: u64,
    name: &'static str,
}

/// The kept-alive client end and its response buffer.
struct Client {
    stream: DuplexStream,
    buf: Vec<u8>,
}

impl Client {
    /// One request, read until the response body is complete.
    async fn round_trip(&mut self, request: &[u8]) -> usize {
        let mut read = 0;

        self.stream.write_all(request).await.expect("write");

        loop {
            read += self.stream.read(&mut self.buf[read..]).await.expect("read");

            if let Some(end) = complete(&self.buf[..read]) {
                return end;
            }
        }
    }
}

/// Length of a complete response with a `content-length` header in `bytes`, if there is one.
fn complete(bytes: &[u8]) -> Option<usize> {
    let head = bytes.windows(4).position(|window| window == b"\r\n\r\n")? + 4;
    let text = std::str::from_utf8(&bytes[..head]).ok()?;
    let length = text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;

        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())?
    })?;

    (bytes.len() >= head + length).then_some(head + length)
}

async fn health() -> &'static str {
    "ok"
}

async fn user(Path(id): Path<u64>) -> Negotiated<User> {
    Negotiated::new(User { id, name: "Ada" })
}

fn connect(runtime: &Runtime) -> Client {
    runtime.block_on(async {
        let (memory, connector) = Memory::new(64 * 1024);
        let routes = Router::new()
            .route(Route::new("/health").expect("pattern").get(health))
            .route(Route::new("/users/{id}").expect("pattern").get(user));
        let server = Server::bind(&memory, ()).await.expect("bind");

        tokio::spawn(server.serve(routes));

        Client {
            stream: connector.connect().await.expect("connect"),
            buf: vec![0; 16 * 1024],
        }
    })
}

fn engine(c: &mut Criterion<Measurement>) {
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let mut client = connect(&runtime);

    for (name, request) in CASES {
        let total = measure(
            || (),
            |()| {
                runtime.block_on(async {
                    for _ in 0..REQUESTS {
                        client.round_trip(request).await;
                    }
                })
            },
        );

        println!(
            "alloc engine/h1/{name} (per request, mean of {REQUESTS}): count={} bytes={}",
            total.count / REQUESTS,
            total.bytes / REQUESTS
        );
        bench(
            c,
            "engine/h1",
            name,
            Budget::Report,
            || (),
            |()| runtime.block_on(client.round_trip(request)),
        );
    }
}

criterion_group! { name = benches; config = criterion(); targets = engine }
criterion_main!(benches);
