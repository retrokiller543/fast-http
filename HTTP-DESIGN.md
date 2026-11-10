# Upwell HTTP: design sketch

Status: exploratory, not approved. Nothing here is committed API. Date: 2026-11-09.

## 1. Goals and non-goals

Goals
1. **Guarded endpoints.** Several handlers may share `(method, path)`. A guard inspects the request (query, headers, optionally body) and selects the variant. Each variant has its own, strongly typed signature and response type.
2. **Pluggable transport.** The byte-moving layer (tokio epoll TCP, io_uring, in-memory for tests, later QUIC/UDS) is replaceable without touching routing, layers, DI or discovery.
3. **Compile-time everything that can be.** Route matching, guard ordering, DI wiring and protocol selection are generated or const-evaluated. Runtime does no reflection and no `Box<dyn>` on the hot path.
4. **Layer graph, not only a stack.** Layers nest (as tower does) but may also fork, branch, fall back and join, forming a tree/DAG of flows.
5. **Tower interoperability.** Existing `tower::Layer` / `tower::Service` (tower-http etc.) work via adapters.
6. **Static protocol config.** HTTP/1.1, HTTP/2 and HTTP/3 are individually enabled by a type/const. Disabled protocols are compiled out, including their dependencies where Cargo features allow.
7. **Keeps upwell's value.** Auto-discovery, compile-time DI, typed config, hooks, generated typed clients and CLI continue to work. Ownership of `main` stays with the user; a builder API remains.

Non-goals (for now)
- Being a general purpose, standalone web framework. This is designed for upwell; DI is upwell's, not re-implemented here.
- Competing with hyper on protocol parsing. We reuse hyper/h2/h3 codecs unless a transport forces otherwise (see 3.3).
- Removing axum. The existing axum integration stays as a backend/escape hatch (`into_axum_router()`), at least until this stack is proven.

## 2. Crate layout (proposed)

| Crate | Role |
|---|---|
| `upwell-http-core` | `Request`/`Response`/`Body` aliases, `FromParts`, `FromRequest`, `IntoResponse`, `Guard`, `Endpoint`, `Routes`, rejection types. No I/O. |
| `upwell-http-layer` | Layer graph (`Flow`, `Layer`, combinators), tower adapters. |
| `upwell-http-transport` | `Transport`, `Listener`, `Connection` traits plus the tokio-tcp implementation. |
| `upwell-http-transport-uring` | io_uring implementation (feature-gated, separate crate so its runtime deps never leak). |
| `upwell-http-server` | Protocol engines (h1, h2, h3), connection driver, graceful shutdown, ALPN, `ServerConfig`. |
| `upwell-http-macros` | `#[route]`/`#[endpoint]`, guard parsing, router codegen. |
| `upwell-http-axum` | Compatibility: `into_axum_router()`, axum extractor bridging. |

Each crate has a README (Role, Usage, Internal role, Feature flags) per repo rules.

## 3. Transport layer

### 3.1 Responsibilities
A transport turns an address into a stream of connections and gives each connection byte I/O. It knows nothing about HTTP, TLS framing or routing.

```rust
pub trait Transport: Send + Sync + 'static {
    type Addr: Clone + Send + Sync + 'static;
    type Listener: Listener<Addr = Self::Addr, Conn = Self::Conn>;
    type Conn: Connection;

    fn bind(&self, addr: Self::Addr) -> impl Future<Output = io::Result<Self::Listener>> + Send;
}

pub trait Listener: Send + 'static {
    type Addr;
    type Conn: Connection;

    fn accept(&mut self) -> impl Future<Output = io::Result<(Self::Conn, Self::Addr)>> + Send;
    fn local_addr(&self) -> io::Result<Self::Addr>;
}
```

`Connection` is where readiness-based and completion-based I/O diverge, so it is deliberately a capability, not a single trait (see 3.2).

### 3.2 Two I/O models
- **Readiness-based (tokio epoll/kqueue):** borrowed buffers, `poll_read(&mut [u8])`. Maps onto `hyper::rt::{Read, Write}` directly via `TokioIo`.
- **Completion-based (io_uring):** the kernel owns the buffer until completion, so the API takes ownership: `read(buf: B) -> (Result<usize>, B)`. This does not map cleanly onto `poll_read` with borrowed slices.

```rust
pub trait Connection {
    type Io: ProtocolIo;
    fn into_io(self) -> Self::Io;
    fn peer_addr(&self) -> io::Result<SocketAddr>;
    fn tls_info(&self) -> Option<&TlsInfo>;
}

pub trait ProtocolIo {
    const MODEL: IoModel;
}

pub enum IoModel { Readiness, Completion }
```

`IoModel` is an enum, not a bool, because it is a finite compile-time-known set.

### 3.3 Feeding protocol engines
| Transport model | h1 / h2 engine |
|---|---|
| Readiness | hyper (h1, h2) driven directly. |
| Completion | **Option A:** adapter with an internal copy buffer exposing `hyper::rt::Read/Write` (simple, loses part of the zero-copy benefit). **Option B:** own h1 codec over owned buffers (`httparse` plus a small state machine), reuse `h2` via the adapter. Start with A; B is only justified by benchmarks. |

io_uring wins mostly come from batching syscalls, multishot accept/recv, registered buffers and fixed files. Option A still keeps the accept path, batching and fewer syscalls; it just does not use registered buffers end to end. Benchmarks must decide (and benchmarks require owner approval before running).

### 3.4 The `Send` problem (important)
io_uring runtimes (`tokio-uring`, `monoio`, `glommio`) are thread-per-core and their futures are generally `!Send`. Upwell currently assumes `+ Send` futures and `Arc` components. Choices:
1. **Keep `Send` everywhere** and run io_uring only as a driver that hands completed buffers to a normal multi-thread runtime. Simplest; limits the gains.
2. **Make `Send` a transport capability:** a marker `type Spawn: Spawner` where `Spawner::spawn` takes `F: Future + SendIfRequired`. Handlers are generated once; the bound is chosen by the `Spawner`. Requires carefully avoiding `Send` in trait signatures that are generic over the executor (return-position `impl Future` with conditional `Send` is not expressible directly today; a macro-emitted duplicate bound per mode is the practical route).
3. **Shared-nothing per-core:** each core builds its own component graph copy (`Rc`). Not compatible with singleton DI semantics. Likely out of scope.

Recommendation: ship (1) first, design traits so (2) is possible, treat (3) as research.

Owner leaning (2026-11-09): not sure, but supporting the same flows as either `Send` or `!Send` is expected to add significant complexity. Default to (1): everything `Send`; io_uring is a transport/driver detail behind that bound. Revisit only if benchmarks show the `Send` boundary is the bottleneck.

### 3.5 UDP and HTTP/3
HTTP/3 does not use `Transport::Listener`; it needs a datagram socket. Add a sibling trait:

```rust
pub trait DatagramTransport: Send + Sync + 'static {
    type Socket: DatagramSocket;
    fn bind(&self, addr: SocketAddr) -> impl Future<Output = io::Result<Self::Socket>> + Send;
}
```

`quinn` has its own `Runtime`/`AsyncUdpSocket` abstraction; implementing `DatagramSocket` on top of it lets an io_uring UDP implementation plug in later (`sendmsg`/`recvmsg` with GSO/GRO). Default: quinn's tokio runtime.

## 4. Protocol engines and static configuration

### 4.1 Reality check on "h2 is needed for multipart / streams / websockets"
- Multipart (`multipart/form-data`) is plain HTTP/1.1 body framing. Works on h1.1.
- Streaming responses (SSE, chunked) work on h1.1. Streaming request bodies work on h1.1 (chunked).
- WebSockets are h1.1 `Upgrade` (RFC 6455). Over h2 they need extended CONNECT (RFC 8441), which is rarely used; over h3, RFC 9220.
- What h2/h3 truly add: multiplexing, header compression, full-duplex bidirectional streaming (gRPC-style), server push (deprecated) and, for h3, no TCP head-of-line blocking.

So **removing h2 does not remove multipart/streams/websockets.** It removes multiplexing and true bidirectional streaming.

**Decision (owner, 2026-11-09): h1.1 and h2 are always compiled in, for simplicity; only h3 is optional.** `ServerConfig` therefore has no switch for `Http1`/`Http2`; the `On`/`Off` protocol switch applies to `Http3` only (plus runtime-tunable limits). An h2-only or h1-only profile can be reconsidered later.

### 4.2 Static config shape
Const/type driven, no runtime flag checks on the hot path:

```rust
pub trait ServerConfig: 'static {
    type Http1: ProtocolSwitch;
    type Http2: ProtocolSwitch;
    type Http3: ProtocolSwitch;

    const MAX_HEADER_BYTES: usize = 16 * 1024;
    const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
    const MAX_CONCURRENT_STREAMS: u32 = 128;
    const ALPN: &'static [&'static [u8]];
}

pub trait ProtocolSwitch { const ENABLED: bool; type Engine<T>: ...; }

pub struct On;
pub struct Off;
```

`Off` uses an uninhabited engine type, so every `match` arm for it is eliminated by the compiler and no code for that protocol is instantiated. A derive/macro provides the ergonomic form:

```rust
#[upwell::server_config(http1, http2, http3(udp = "0.0.0.0:443"))]
struct Prod;
```

### 4.3 Cargo features vs types
Types/consts remove *code paths*; they do not remove *dependencies from the dependency graph*. Cargo features are additive and global, so:
- `h2`, `h3` Cargo features control whether `h2`, `quinn`, `h3` crates are linked at all (build time and binary size).
- `ServerConfig` selects which compiled-in protocols a particular server uses. Enabling a protocol type whose feature is off is a compile error with a clear message.

### 4.4 Connection driver
Per accepted connection (readiness model shown):
1. Optional TLS accept (`tokio-rustls`; no openssl). ALPN result selects the engine: `h2`, `http/1.1`.
2. Cleartext: h1.1 by default; optional **h2c prior knowledge** (config `const H2C: bool`). The `Upgrade: h2c` path is deprecated and not supported.
3. The engine yields `(Parts, Incoming)` pairs and expects a `Response` future per request from `Routes::dispatch`.

### 4.4b QUIC / HTTP/3 library options

Goal: protocol logic only, with us owning sockets (so a custom or io_uring UDP path works) and no bundled framework.

| Option | Shape | Notes |
|---|---|---|
| `quiche` (Cloudflare) | Sans-IO QUIC + HTTP/3 (`quiche::h3`). We feed datagrams in, drain datagrams out, and drive timers. | Closest to "protocol only". Ideal for io_uring/custom UDP. Production-proven. Default TLS is vendored BoringSSL (C++/cmake build, slower builds, must verify against the no-openssl rule; BoringSSL is not the `openssl` crate, but `cargo tree` must be checked). We write the connection/stream event loop ourselves. |
| `tokio-quiche` (Cloudflare) | Async wrapper around quiche for tokio. | Convenient, but ties UDP I/O to tokio, which defeats the pluggable-transport goal for UDP. |
| `quinn-proto` | Sans-IO core of quinn, rustls-based. | Pure Rust, rustls only. No H3 framing, so pair with `h3`. Sans-IO means we own the UDP socket. |
| `h3` (hyperium) | HTTP/3 only, generic over a QUIC trait; `h3-quinn` is the adapter. | Protocol-only H3 over any QUIC impl that satisfies its traits. Writing our own `h3` QUIC adapter over `quinn-proto` or quiche is possible. |
| `s2n-quic` (AWS) | Full QUIC, own runtime abstraction; has `s2n-quic-h3`. | Pure Rust option with a pluggable I/O provider. Heavier than the others. |

Leading candidates: (a) `quinn-proto` + `h3` with our own UDP socket glue (pure Rust, rustls, fits the no-openssl rule); (b) `quiche` if its performance/maturity justifies the BoringSSL build cost. Decide with a spike comparing build time, binary size and throughput. Keep the choice behind our `DatagramTransport` / H3 engine trait so it stays swappable.

### 4.5 HTTP/3 specifics
- Requires TLS 1.3 (QUIC). Config must reject `Http3 = On` without TLS.
- Starts a parallel UDP socket, same port number by convention as TCP/443.
- Servers advertise it via `Alt-Svc: h3=":443"; ma=86400` on h1/h2 responses. Generated by the server layer from config; opt-out const.
- Separate accept loop and shutdown path; the shared `Routes::dispatch` is protocol-agnostic.
- Optional 0-RTT is off by default (replay risk); guards can see `is_early_data` if enabled.

## 5. Request pipeline

```
Transport -> (TLS) -> Protocol engine -> Flow graph (layers) -> Routes::dispatch -> Guards -> Endpoint -> IntoResponse
```

The `Flow` graph wraps `Routes::dispatch` and can also appear per route or per endpoint (see 7).

## 6. Core traits (handler side)

```rust
pub type Request<B = Incoming> = http::Request<B>;
pub type Response<B = ResponseBody> = http::Response<B>;

pub trait IntoResponse { fn into_response(self) -> Response; }

pub trait FromParts: Sized {
    type Rejection: IntoResponse;
    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection>;
}

pub trait FromRequest: Sized {
    type Rejection: IntoResponse;
    fn from_request(parts: &Parts, body: Incoming)
        -> impl Future<Output = Result<Self, Self::Rejection>> + Send;
}
```

Rules
- Any number of `FromParts` arguments; at most one `FromRequest`, and it must be last. Enforced by the macro (compile error with span on the offending argument), not by trait-system trickery.
- Rejections are strictly typed per extractor; statuses use `http::StatusCode` (opaque to the framework, no `retryable()` etc.).
- `ResponseBody` is an enum: `Empty | Bytes(Bytes) | Stream(...)`; exactly one boxed variant lives at the streaming boundary.

### 6.1 Guards

```rust
pub trait Guard {
    const PHASE: GuardPhase;
    type Output;

    fn check(parts: &Parts) -> Option<Self::Output>;
}

pub enum GuardPhase { Head, Body }
```

- **Head guards** are sync, allocation-free, run on `Parts`: query value, header, `Accept`, path param predicate, HTTP version, peer info.
- **Body guards** are async and need the payload (e.g. JSON field `stream`). The server buffers up to a limit (`MAX_BODY_BYTES`, per-endpoint override), parses once, and hands the parsed value *and* the re-materialized body to the chosen variant so the work is not repeated. Streaming-upload endpoints cannot use body guards; the macro rejects the combination at compile time.
- `Guard::Output` is the typed value forced onto the handler. Example: `when(query.stream = true)` yields `Stream` (a ZST marker or parsed value), and the handler signature is written against it.

Usage sketch

```rust
#[post("/chat", when(query.stream = false))]
async fn chat(&self, req: Json<ChatReq>) -> Json<ChatRes> { ... }

#[post("/chat", when(query.stream = true))]
async fn chat_stream(&self, req: Json<ChatReq>) -> Sse<ChatChunks> { ... }
```

`&self` is an upwell component; its dependencies are injected at assembly (section 8).

### 6.2 Endpoint and routes

```rust
pub trait Endpoint: Send + Sync + 'static {
    const METHOD: http::Method;
    const PATH: &'static str;
    const PRIORITY: u16;
    const PHASE: GuardPhase;

    fn matches(&self, parts: &Parts) -> bool;
    fn call(&self, parts: Parts, body: Incoming) -> impl Future<Output = Response> + Send;
}

pub trait Routes: Send + Sync + 'static {
    fn dispatch(&self, req: Request) -> impl Future<Output = Response> + Send;
}
```

**Builder (implemented):**

```rust
Router::new()
    .route(Route::new("/users/{id}")?.get(show).put(update))
    .merge(orgs())
```

- Each call appends statically through `TuplePush`: a route's endpoints form a tuple of up to 16.
- A router's route list grows like a base-16 counter (`Level<Digit, Up>`). Full 16-tuples carry upward, so `n` routes nest only log16(n) levels deep. That keeps future nesting far below rustc's layout-depth limit; a flat chain hit it at about 500 routes.
- A single `.route` chain still costs quadratic compile time and debug stack, because each step is a bigger type. Large apps should build per-controller groups and `merge` them, which is also the shape generated registration takes.

## 7. Layer graph

### 7.1 Why not just a stack
Tower composes linearly: `Layer<S> -> S`. We also want:
- **Branch:** choose a sub-flow by guard (per-protocol, per-tenant, per-`Accept`).
- **Fallback:** try flow A; on a typed rejection, continue to B.
- **Tee / shadow:** send a copy of the request to a secondary flow, ignore its result (mirroring, audit).
- **Join / merge:** several flows converging on one downstream (shared auth after multiple front ends).
- **Scoped attachment:** attach a flow to a path prefix, an endpoint, or the whole server, with inheritance from parent to child like a tree.

### 7.2 Core abstraction

```rust
pub trait Flow: Send + Sync + 'static {
    fn handle<N: Next>(&self, req: Request, next: N)
        -> impl Future<Output = Response> + Send;
}

pub trait Next: Send {
    fn run(self, req: Request) -> impl Future<Output = Response> + Send;
}
```

- `Flow` is the layer. `Next` is the rest of the pipeline, passed by value and statically typed (no boxing).
- A **stack** is `Chain<A, B>`; `Chain<A,B>::handle` calls `A.handle(req, BThen<next>)`.
- Graph nodes are ordinary `Flow` implementors built from combinators:

```rust
Branch<G: Guard, A: Flow, B: Flow>
Fallback<A: Flow, B: Flow>
Tee<Primary: Flow, Shadow: Flow>
Scope<Prefix, F: Flow>
```

- The graph is a *type*, assembled by the macro/builder; discovery collects `#[layer]` items with scope metadata (global, prefix, endpoint) and the generated assembly nests them into a single concrete type per endpoint. No runtime graph walking.
- Responses flow back through the same nodes (like tower), so layers can post-process.

### 7.3 Dynamic flow
For genuinely runtime decisions (feature flags, config reload) provide an explicit `DynFlow` boxed node. It is opt-in and never inserted by default, so the static path stays unboxed.

### 7.4 Ordering rules
- Outer to inner: global, then prefix scopes by depth, then endpoint layers.
- Within a scope: explicit `order = N`; ties broken by declaration order in discovery (stable). The generated assembly emits the final order as a const table so it can be asserted in tests.

### 7.5 Errors
Layers return `Response`; fallible internals use typed errors (`thiserror`) converted at the layer boundary by an `IntoResponse` of the layer's own error enum. No `anyhow`.

## 8. DI integration

- Upwell's compile-time DI is unchanged. An endpoint struct is generated per `#[route]` and holds the fields it needs (`Arc<Component>` or whatever the component handle type is) resolved at graph build.
- Handler arguments fall into three classes, decided by the macro:
  1. `&self` / component fields: injected at assembly, zero per-request cost.
  2. `FromParts` / `FromRequest`: per-request extraction.
  3. Guard outputs: typed values produced by the matched guard.
- Layers are components too and can receive DI at construction. A `Flow` is a plain struct: Upwell builds it in its container with its dependencies as fields (`&self`), then registers it with `.layer(flow)`. Through those fields it can put request data, such as the user or session, into extensions. fast-http needs no extra API for this.
- `Server::serve(routes)` runs until the listener fails or the process ends; `.graceful_shutdown(signal)` is an optional builder step that stops accepting when `signal` resolves and drains within the deadline.
- Context propagation: `Server::task_wrap(w)` takes a `TaskWrap` (static generic, `Plain` by default) that wraps every task the server spawns, both connection tasks and the engine's HTTP/2 stream tasks. Upwell implements it with `CapturedContext::bind`, so handlers and flows see the context that was current where the server was started.
- **Deferred: request-scoped injection into handler arguments.** fast-http stays a plain HTTP framework with no DI of its own. Extractors other than the body are synchronous (`FromParts`/`FromParams`), and adding async, container-backed extractors would put a second, axum-style DI system next to Upwell's. For now, request-scoped values reach handlers through Upwell's `Current<T>`/`Scoped<S, T>` (the scope entered by a flow) or through extensions populated by a flow. The decision can be revisited if a design keeps fast-http simple.
- No `Resolver` trait changes are required for this design.

## 9. Router codegen

For the discovered endpoint set, the macro emits one `Routes` implementation:
1. Group by `(METHOD, PATH)` preserving priorities.
2. Compile paths into a nested `match` over segments; path params parsed into typed fields in generated code (errors are 400 with typed rejection).
3. Within a group, check `PHASE = Head` guards in priority order; if any `Body` guards exist, buffer once, then check them.
4. No match: 404 if the path is unknown; 405 with `Allow` (derived at compile time) if method differs; 400/422 (configurable) if the path+method exists but all guards failed.
5. Compile errors for: two variants with identical guards, a streaming-body endpoint with body guard, unreachable variants after a catch-all.
6. HEAD and OPTIONS derived from GET and the `Allow` set unless overridden.

Nothing is generic over handler tuples, so the type checker has no inference to do per handler. This is the main build-time advantage over axum's blanket `Handler for Fn(A, B, C, ...)` impls and deeply nested tower types.

## 10. Tower compatibility

### 10.1 Using tower layers in our graph
```rust
pub struct TowerFlow<L>(pub L);
```
`TowerFlow<L>` implements `Flow` by wrapping `Next` in an adapter that implements `tower::Service<Request<ReqBody>>` (`poll_ready` always `Ready`, `call` runs `next`), applying `L`, then calling the resulting service. Constraints to handle:
- `Service::call(&mut self)` and `Clone` expectations: the adapter is cheap to clone (it holds the `Next` by value, so clone requires `N: Clone`; fall back to a once-cell for single-use `Next`).
- Body type: tower-http layers are generic over `http_body::Body`; our `Incoming`/`ResponseBody` implement `http_body::Body`, and error types are mapped to `Infallible`/typed errors at the boundary.
- `poll_ready` back-pressure semantics (e.g. `ConcurrencyLimit`) are honored by awaiting `poll_ready` inside `handle` before `call`.

### 10.2 Using our flows as tower layers
`FlowLayer<F>` implements `tower::Layer<S>` so a `Flow` can be placed in an axum/hyper tower stack (axum bridge).

### 10.3 Limits
Tower stacks are linear. A tower layer slots into one node; graph shape comes from our combinators. Layers that expect to see the *outermost* request (e.g. `NormalizePath`) must be scoped globally.

## 11. WebSockets, multipart, streaming

- **WebSocket (h1.1):** the `Upgrade` flow returns `101` and hands the raw connection (`Upgraded`) to a `WebSocket` handler type. Requires the transport `Connection::Io` to expose a hijack-capable path; the engine gives back the buffered read remainder plus the IO. With completion-based transports this needs the adapter to expose unread buffered bytes.
- **WebSocket over h2/h3 (RFC 8441 / 9220):** extended CONNECT, deferred; feature gated per protocol.
- **Multipart:** `FromRequest` implementation over `http_body::Body`, streaming field-by-field with per-field and total size limits from config. Works on every protocol.
- **Streaming responses:** `ResponseBody::Stream`; SSE is a helper type over it. Back-pressure is the engine's responsibility.
- **Full-duplex request/response streaming:** only advertised on h2/h3; the macro rejects such endpoints when the configured protocol set lacks them (compile error, since config is static).

## 12. Lifecycle, limits, observability

- **Graceful shutdown:** a shutdown token stops accept loops (TCP and UDP), engines send GOAWAY (h2/h3) or `Connection: close` (h1), and drain with a deadline. Integrates with upwell lifecycle hooks.
- **Limits (all const in `ServerConfig`):** header size/count, body size, idle/keep-alive timeouts, request header read timeout (slowloris), max concurrent connections and streams, h2 reset-flood and rapid-reset protections (rely on `h2` crate mitigations; verify versions).
- **Logging:** `tracing` with explicit targets `upwell::http::transport`, `upwell::http::server`, `upwell::http::router`, `upwell::http::layer`.
- **Metrics hooks:** connection counts, per-protocol request counters, guard-miss counters; defined as a trait, not a fixed backend.

## 13. Build-time and binary-size strategy

1. Proc-macro generates one concrete `Routes` and one concrete layer type per endpoint; no blanket-impl inference.
2. Non-generic inner functions for shared work (rejection rendering, header parsing, limit checks); generated code is thin shims over them.
3. `ServerConfig` associated types/consts eliminate unused engines before monomorphization where possible.
4. Cargo features for `h2`, `h3`, `uring`, `tower-compat`, `axum-compat` keep heavy deps out of default builds.
5. Dyn only at explicit seams: `DynFlow`, the single boxed stream variant, and optional dynamic transports for tests.
6. Track: `cargo build --timings` and `cargo llvm-lines` for the generated route crate as regression indicators.

## 14. Testing strategy

- **In-memory transport** (duplex streams) as a first-class `Transport` for fast tests: no sockets, deterministic.
- **Conformance suite** parameterized by `Transport` and protocol: run the same request matrix (guards, streaming, multipart, upgrade, shutdown) over tokio-tcp and io_uring.
- **Compile-fail tests** (`trybuild`) for macro diagnostics: duplicate guards, body-extractor-not-last, streaming endpoint with body guard, protocol/feature mismatch.
- **Interop:** curl (h1, h2, h3), `h2spec`, `h3spec` or equivalent, and a tower-http layer matrix.
- **Layer graph tests:** assert the generated const ordering table.
- Use `cargo nextest run`, no retries; benchmarks only after owner approval.

## 15. Phased roadmap

1. **Phase 0 (spike):** `Guard`, `Endpoint`, generated `Routes` for head guards only on top of hyper + tokio TCP, h1.1 only. Validate the `/chat?stream=` example and compile-time cost against the axum equivalent.
2. **Phase 1:** `Flow`/`Next`, `Chain`, `Branch`, `Fallback`, tower adapter, tower-http smoke tests.
3. **Phase 2:** `ServerConfig` with `On`/`Off` protocols, h2 + TLS/ALPN, graceful shutdown, upwell DI integration and discovery.
4. **Phase 3:** body guards, multipart, SSE, WebSocket (h1.1).
5. **Phase 4:** h3 over quinn with Alt-Svc.
6. **Phase 5:** io_uring transport (Option A adapter), `Send` strategy decision (3.4), benchmarks (owner-approved).
7. **Phase 6:** typed client generation for guard variants (`chat()` and `chat_stream()` setting the right query/header), OpenAPI emission (variants as one operation with `oneOf` responses plus vendor extension).

## 16. Open questions

1. `Send` strategy for io_uring (3.4): does upwell want to support `!Send` handler futures at all?
2. Is the completion-model adapter acceptable (Option A), or do we own an h1 codec (Option B)?
3. Body-guard buffering limits: global default and per-endpoint override syntax.
4. Guard miss status: 400 vs 404 vs 422 default, and whether it is configurable per route.
5. h1.1 always-on: confirm, or support an h2-only profile (affects `ServerConfig` shape).
6. How far should `FromParts`/`FromRequest` mirror axum names, for familiarity and easier porting?
7. Should the axum compatibility crate be permanent, or a migration aid with a removal date?
8. OpenAPI 3.x cannot express "same operation, different query-selected variants"; accept `oneOf` plus extension, or emit synthetic operations with `x-` hints?
9. New dependencies (hyper-util, h2, h3/quinn, tokio-rustls, io_uring runtime) all need owner approval and a `cargo tree` check that nothing pulls `openssl` or `rsa`.
