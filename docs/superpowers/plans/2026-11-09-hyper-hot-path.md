# Hyper Hot Path Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make fast-http measurably cheaper per request than axum and actix-web while keeping hyper as the engine.

**Architecture:** First make the load harness trustworthy (CPU time per request, repeats, pinned Linux runs) and add an in-process engine allocation bench. Then clean up the shutdown API, remove per-request allocations and copies in the server and router layers, and tune hyper's options. Every optimisation is kept only if a bench shows a gain.

**Tech Stack:** Rust 2024, tokio, hyper 1.12 / hyper-util 0.1.21, Criterion + allocation-counter, oha, sysinfo 0.40, Docker for the pinned Linux run.

**Spec:** `docs/superpowers/specs/2026-11-09-hyper-hot-path-design.md`

## Global Constraints

- Keep hyper; no custom engine and no thread-per-core mode in this plan.
- Handlers keep receiving `http::Request`.
- Ask before adding any dependency; never add `openssl` or `rsa`. Competitor and harness deps stay in `benchmarks/` only.
- Errors are typed with `thiserror`.
- Formatting: a blank line after every block, variables grouped at the top of a scope, the trailing expression isolated. No comments in bodies except safety notes, reasons for ignored errors and bug notes. Every struct, enum and trait has a `///` doc.
- Tests live in a sibling `tests.rs` (`#[cfg(test)] mod tests;`); run them with `cargo nextest`, never `cargo test` (doctests excepted).
- Never run servers, `just load`, `just bench` or profiles without the user's approval; compile-only checks are fine.
- Gate before every commit: `just fmt-check`, `just clippy`, `just test`, `just test-benchmarks`, `just deny`, `just check-isolated`.
- Commits go through `mise exec -- git commit`, Conventional Commits.
- Each optimisation task (6–12) reverts its change if the named measurement shows no gain or a regression, and records the numbers in the commit message.

---

### Task 1: CPU time per request in the load harness

**Files:**
- Modify: `benchmarks/load/src/harness/rss.rs`
- Modify: `benchmarks/load/src/harness/rss/tests.rs`
- Modify: `benchmarks/load/src/harness/mod.rs` (`measure`)
- Modify: `benchmarks/load/src/harness/report.rs` (`Measured`, markdown, `rss.csv`)

**Interfaces:**
- Produces: `Sample { at_ms: u64, rss: u64, cpu_ms: u64 }`; `cpu_per_request(samples: &[Sample], start_ms: u64, end_ms: u64, responses: u64) -> Option<f64>` (microseconds); `Measured::cpu_us: Option<f64>`.

- [ ] **Step 1: Write the failing test** in `rss/tests.rs`:

```rust
#[test]
fn cpu_per_request_divides_cpu_spent_in_the_window_by_responses() {
    let samples = [
        Sample { at_ms: 0, rss: 1, cpu_ms: 100 },
        Sample { at_ms: 1_000, rss: 1, cpu_ms: 400 },
        Sample { at_ms: 2_000, rss: 1, cpu_ms: 900 },
    ];

    assert_eq!(cpu_per_request(&samples, 0, 2_000, 1_000), Some(800.0));
    assert_eq!(cpu_per_request(&samples, 0, 2_000, 0), None);
    assert_eq!(cpu_per_request(&samples, 500, 600, 10), None);
}
```

Also add `cpu_ms: 0` to every existing `Sample` literal in the file.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo nextest run --manifest-path benchmarks/Cargo.toml -p fast-http-load cpu_per_request`
Expected: compile error, `cpu_ms` and `cpu_per_request` do not exist.

- [ ] **Step 3: Implement.** In `rss.rs` add the field, the function, and read CPU time in the sampler:

```rust
/// One reading of the server process.
pub struct Sample {
    /// Milliseconds since the server started.
    pub at_ms: u64,
    /// Resident set size in bytes.
    pub rss: u64,
    /// CPU time the process has used so far, in milliseconds, all threads together.
    pub cpu_ms: u64,
}

/// Server CPU microseconds per response between the first and last sample in
/// `[start_ms, end_ms]`; `None` without two samples or without responses.
pub fn cpu_per_request(samples: &[Sample], start_ms: u64, end_ms: u64, responses: u64) -> Option<f64> {
    let mut within = samples
        .iter()
        .filter(|sample| (start_ms..=end_ms).contains(&sample.at_ms));
    let first = within.next()?;
    let last = within.last()?;

    if responses == 0 {
        return None;
    }

    Some((last.cpu_ms - first.cpu_ms) as f64 * 1_000.0 / responses as f64)
}
```

In `sample()`, refresh CPU as well and record it:

```rust
let kind = ProcessRefreshKind::nothing().with_memory().with_cpu();
// ...
samples.push(Sample {
    at_ms: u64::try_from(origin.elapsed().as_millis()).unwrap_or(u64::MAX),
    rss: process.memory(),
    cpu_ms: process.accumulated_cpu_time(),
});
```

Confirm `Process::accumulated_cpu_time` exists in sysinfo 0.40 and returns milliseconds (`grep -n "fn accumulated_cpu_time" -A5 ~/.cargo/registry/src/*/sysinfo-0.40*/src/common/system.rs`); if its unit differs, convert to milliseconds here.

- [ ] **Step 4: Wire it into the report.** In `report.rs` add `pub cpu_us: Option<f64>` to `Measured` (doc: "Server CPU microseconds per response."). In `harness/mod.rs::measure`, next to `load.memory = ...`:

```rust
load.cpu_us = cpu_per_request(
    &samples,
    load.phase.start_ms,
    load.phase.end_ms,
    load.outcome.responses,
);
```

Set `cpu_us: None` in `measured()`. In the markdown latency table add a `cpu µs/req` column right after `req/s` (format `{:.1}`, `-` when `None`; bump the `separator` count by one). In `rss.csv` add a `cpu_ms` column.

- [ ] **Step 5: Run the tests**

Run: `just test-benchmarks && cargo clippy --manifest-path benchmarks/Cargo.toml --workspace --all-targets -- -D warnings`
Expected: all pass, no warnings.

- [ ] **Step 6: Commit**

```bash
mise exec -- git commit -am "feat(load): report server CPU time per request"
```

---

### Task 2: Repeats with rotated framework order

**Files:**
- Modify: `benchmarks/load/src/harness/config.rs` (+ `config/tests.rs`)
- Modify: `benchmarks/load/src/harness/mod.rs` (`run`)
- Modify: `benchmarks/load/src/harness/report.rs`
- Create: `benchmarks/load/src/harness/report/tests.rs` if it does not exist

**Interfaces:**
- Consumes: `Measured::cpu_us` (Task 1).
- Produces: `Config::repeat: u32` (default 1, flag `--repeat N`, rejected when 0); `FrameworkReport::repeat: u32`; `Report::frameworks` holds one entry per framework per repeat; `pub fn rotated(frameworks: &[Framework], repeat: u32) -> Vec<Framework>`; `Spread { median: f64, min: f64, max: f64 }` and `fn spread(values: &mut [f64]) -> Option<Spread>`.

- [ ] **Step 1: Write the failing tests.** In `config/tests.rs`:

```rust
#[test]
fn repeat_is_parsed_and_zero_is_rejected() {
    let config = Config::parse(args(&["--repeat", "3"])).expect("valid");

    assert_eq!(config.repeat, 3);
    assert!(Config::parse(args(&["--repeat", "0"])).is_err());
}
```

(Use the existing argument helper in that file; if there is none, `fn args(list: &[&str]) -> Vec<String> { list.iter().map(|s| (*s).to_owned()).collect() }`.)

In `harness/tests.rs` (create; add `#[cfg(test)] mod tests;` to `harness/mod.rs`):

```rust
use super::*;

#[test]
fn every_repeat_starts_with_the_next_framework() {
    let all = [Framework::FastHttp, Framework::Axum, Framework::Actix];

    assert_eq!(rotated(&all, 0), all);
    assert_eq!(rotated(&all, 1), [Framework::Axum, Framework::Actix, Framework::FastHttp]);
    assert_eq!(rotated(&all, 3), all);
}
```

In `report/tests.rs`:

```rust
use super::*;

#[test]
fn spread_takes_median_and_range() {
    let spread = spread(&mut [3.0, 1.0, 2.0, 10.0]).expect("values");

    assert_eq!((spread.median, spread.min, spread.max), (2.5, 1.0, 10.0));
    assert!(super::spread(&mut []).is_none());
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `just test-benchmarks`
Expected: compile errors for `repeat`, `rotated` and `spread`.

- [ ] **Step 3: Implement.**
  - `config.rs`: add `/// Times the whole matrix runs, each time starting with the next framework.\n pub repeat: u32,` with default 1, the `--repeat N` usage line (`--repeat N            runs of the whole matrix, rotating the framework order (default: 1)`), parsing, and a `ConfigError` for 0, following how the other numeric flags are parsed and rejected in that file.
  - `harness/mod.rs`:

```rust
/// The frameworks in the order repeat `repeat` runs them.
pub fn rotated(frameworks: &[Framework], repeat: u32) -> Vec<Framework> {
    let mut order = frameworks.to_vec();

    if !order.is_empty() {
        order.rotate_left(repeat as usize % order.len());
    }

    order
}

pub fn run(config: &Config) -> Result<Report, LoadError> {
    let mut frameworks = Vec::new();

    for repeat in 0..config.repeat {
        for framework in rotated(&config.frameworks, repeat) {
            let mut report = measure(config, framework)?;

            report.repeat = repeat;
            frameworks.push(report);
        }
    }

    Ok(Report {
        config: config.clone(),
        frameworks,
    })
}
```

  Print `[<framework> #<repeat>]` in `Timeline::record` once `repeat > 0` exists in the timeline (add a `repeat: u32` field to `Timeline`).
  - `report.rs`:

```rust
/// Median and range of one measurement over the repeats.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Spread {
    /// Median value.
    pub median: f64,
    /// Smallest value.
    pub min: f64,
    /// Largest value.
    pub max: f64,
}

fn spread(values: &mut [f64]) -> Option<Spread> {
    values.sort_by(f64::total_cmp);

    let (min, max) = (*values.first()?, *values.last()?);
    let middle = values.len() / 2;
    let median = if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    };

    Some(Spread { median, min, max })
}
```

  In the markdown latency table, group by (profile, scenario, framework) across repeats: print the median for every numeric column, and for `req/s` and `cpu µs/req` print `median (min–max)` when `repeat > 1`. Memory and allocation tables use the median likewise. Add `repeat` as the second column of `rss.csv`. `summary.json` keeps every repeat unaggregated.

- [ ] **Step 4: Run the tests and lint**

Run: `just test-benchmarks && just clippy`
Expected: pass, no warnings.

- [ ] **Step 5: Update `benchmarks/load/README.md`** (usage example `just load --repeat 3`, `rss.csv` columns, the `cpu µs/req` column and why it is the primary metric) and commit:

```bash
mise exec -- git commit -am "feat(load): repeat runs with rotated framework order and report spreads"
```

---

### Task 3: Pinned Linux run

**Files:**
- Modify: `benchmarks/Cargo.toml` (target-specific `libc`, **ask the user first**)
- Modify: `benchmarks/load/Cargo.toml`
- Modify: `benchmarks/load/src/serve/mod.rs` (`Options`, `runtime`)
- Create: `benchmarks/load/src/serve/pin.rs`
- Modify: `benchmarks/load/src/bin/serve-actix.rs`
- Modify: `benchmarks/load/src/harness/{config.rs,server.rs,oha.rs}`
- Create: `benchmarks/load/Dockerfile`
- Modify: `justfile` (`load-linux`)

**Interfaces:**
- Produces: `Options::cpus: Option<Vec<usize>>` (`--cpus 0,1,2`); `Config::server_cpus` and `Config::client_cpus` (`--server-cpus LIST`, `--client-cpus LIST`), passed to the server binary as `--cpus` and to oha by wrapping it in `taskset -c LIST` on Linux; `pin::current_thread(cpus: &[usize])` (no-op off Linux).

- [ ] **Step 1: Ask the user** to approve `libc = "0.2"` as a `[target.'cfg(target_os = "linux")'.dependencies]` entry of `fast-http-load` only (it is already in `benchmarks/Cargo.lock` through tokio; check with `cargo tree --manifest-path benchmarks/Cargo.toml -i libc`). Stop here if declined and pin with `taskset` around the server process instead (Step 3 alternative).

- [ ] **Step 2: Write the failing test** in `serve/tests.rs` (create, add `#[cfg(test)] mod tests;` to `serve/mod.rs`):

```rust
use super::*;

#[test]
fn cpus_flag_is_a_comma_list() {
    let options = Options::parse(["--workers", "2", "--cpus", "0,2"].map(str::to_owned)).expect("valid");

    assert_eq!(options.cpus, Some(vec![0, 2]));
    assert!(Options::parse(["--cpus", "a"].map(str::to_owned)).is_err());
}
```

Split the existing `Options::from_args` into `from_args()` (reads `std::env::args().skip(1)`) and `parse(args: impl IntoIterator<Item = String>)`.

- [ ] **Step 3: Implement pinning.** `serve/pin.rs`:

```rust
//! Pins the calling thread to a set of CPUs; a no-op where the OS has no affinity API.

/// Pins the calling thread to `cpus`.
#[cfg(target_os = "linux")]
pub fn current_thread(cpus: &[usize]) {
    // SAFETY: `set` is a zeroed, valid cpu_set_t; CPU_SET only writes inside it and
    // sched_setaffinity reads `size_of::<cpu_set_t>()` bytes from it.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();

        for &cpu in cpus {
            libc::CPU_SET(cpu, &mut set);
        }

        // Pinning is best effort: an unpinned run is still valid, only noisier.
        let _ = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
    }
}

/// Pins the calling thread to `cpus`.
#[cfg(not(target_os = "linux"))]
pub fn current_thread(_: &[usize]) {}
```

In `serve::runtime(workers, cpus: Option<Vec<usize>>)` add `.on_thread_start(move || pin::current_thread(&cpus))` when `Some`. actix-web starts its own worker threads, so `serve-actix.rs` pins inside the `HttpServer::new` app factory, which actix runs once on each worker thread: `HttpServer::new(move || { pin::current_thread(&cpus); App::new().configure(apps::actix) })`.

Alternative when `libc` is declined: `ServerProcess::spawn` runs `taskset -c LIST serve-<framework>` on Linux.

- [ ] **Step 4: Harness flags.** `Config` gets `server_cpus: Option<String>` and `client_cpus: Option<String>` (`--server-cpus 0-3`, `--client-cpus 4-7`, passed through verbatim). `ServerProcess::spawn` appends `--cpus <list>` (expand `a-b` ranges in `Options::parse`; add a case to the Step 2 test: `"0-2"` → `[0, 1, 2]`). `oha::run` builds `Command::new("taskset").args(["-c", list, "oha"])` instead of `Command::new("oha")` when `client_cpus` is set and `cfg!(target_os = "linux")`; on macOS reject the flags with a `ConfigError` ("CPU pinning needs Linux").

- [ ] **Step 5: Dockerfile and recipe.** `benchmarks/load/Dockerfile`:

```dockerfile
FROM rust:1-bookworm
RUN cargo install oha --locked && apt-get update && apt-get install -y --no-install-recommends util-linux && rm -rf /var/lib/apt/lists/*
WORKDIR /src
```

`justfile`:

```just
# Runs the load comparison in a Linux container with server and oha pinned to separate cores.
[doc("Load comparison in Docker with pinned CPUs; `just load-linux 0-3 4-7 --repeat 3`.")]
load-linux server_cpus client_cpus *args:
    docker build -q -t fast-http-load benchmarks/load
    docker run --rm -v "{{ justfile_directory() }}:/src" -v fast-http-load-target:/src/benchmarks/target fast-http-load \
        bash -c "cargo build --release --manifest-path benchmarks/Cargo.toml -p fast-http-load --bins && \
        benchmarks/target/release/load --output benchmarks/load/results --server-cpus {{ server_cpus }} --client-cpus {{ client_cpus }} {{ args }}"
```

The named volume keeps Linux build artefacts out of the host's `benchmarks/target`.

- [ ] **Step 6: Verify (compile only)**

Run: `just test-benchmarks && just clippy && docker build -q -t fast-http-load benchmarks/load`
Expected: pass. Do not run `just load-linux` without approval.

- [ ] **Step 7: README section "Pinned Linux runs"** in `benchmarks/load/README.md`, then commit:

```bash
mise exec -- git commit -am "feat(load): pin server and oha to separate CPUs on Linux"
```

---

### Task 4: Server profiling recipe

**Files:**
- Modify: `justfile`
- Modify: `benchmarks/load/README.md`

- [ ] **Step 1: Add the recipe.** It starts one server, drives it with oha for 15 s, samples it for 10 s and stops it:

```just
# Writes `benchmarks/load/results/profile-<framework>-<scenario>.txt` (macOS `sample`) or `.data`
# (Linux `perf record`); scenario is a path such as `/users/42`.
[doc("CPU profile of one server under load; `just profile-server fast-http /users/42`.")]
profile-server framework path="/health":
    #!/usr/bin/env bash
    set -euo pipefail

    cargo build --manifest-path benchmarks/Cargo.toml --profile profiling -p fast-http-load --bins
    mkdir -p benchmarks/load/results
    out="benchmarks/load/results/profile-{{ framework }}-$(echo '{{ path }}' | tr '/' '_')"
    exec 3< <(benchmarks/target/profiling/serve-{{ framework }} --workers 4)
    read -r _ _ addr _ <&3
    pid=$(pgrep -n "serve-{{ framework }}")
    trap 'kill "$pid"' EXIT
    oha --no-tui -c 64 -z 15s "http://$addr{{ path }}" > /dev/null &
    sleep 3

    if [[ "$(uname)" == Darwin ]]; then
        sample "$pid" 10 -file "$out.txt"
    else
        perf record -g -p "$pid" -o "$out.data" -- sleep 10
    fi

    wait
```

Add to `benchmarks/Cargo.toml`:

```toml
[profile.profiling]
inherits = "release"
debug = true
```

Confirm the ready line format against `serve::announce` (`listening <app addr> <control addr>`) and adjust the `read` fields if needed.

- [ ] **Step 2: Verify compile only**

Run: `cargo build --manifest-path benchmarks/Cargo.toml --profile profiling -p fast-http-load --bins && just --dry-run profile-server fast-http`
Expected: builds; dry run prints the script.

- [ ] **Step 3: Document and commit**

```bash
mise exec -- git commit -am "build: add server profiling recipe"
```

---

### Task 5: Optional graceful shutdown

**Files:**
- Modify: `crates/server/src/server/mod.rs`
- Modify: `crates/server/src/server/tests.rs`
- Modify: `crates/server/src/testing.rs`
- Modify: `crates/server/examples/hello.rs`
- Modify: `benchmarks/load/src/bin/serve-fast-http.rs`
- Modify: `crates/server/README.md`, `HTTP-DESIGN.md` §8

**Interfaces:**
- Produces: `Server<L, E = Hyper, W = Plain, S = Pending<()>>`; `Server::graceful_shutdown<T: Future<Output = ()> + Send>(self, signal: T) -> Server<L, E, W, T>`; `Server::serve<R: Routes>(self, routes: R)`.

- [ ] **Step 1: Write the failing test** in `server/tests.rs`:

```rust
#[tokio::test]
async fn serves_without_a_shutdown_signal() {
    let (memory, connector) = Memory::new(64 * 1024);
    let server = Server::bind(&memory, ()).await.expect("bind");
    let task = tokio::spawn(server.serve(testing::routes()));
    let mut sender = testing::h1(connector.connect().await.expect("connect")).await;
    let response = sender
        .send_request(testing::request(Method::GET, "/health", b""))
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    task.abort();
}
```

Use whichever health-style route `testing::routes()` provides; make `routes` `pub` in `testing.rs` if it is not.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo nextest run -p fast-http-server serves_without_a_shutdown_signal`
Expected: compile error, `serve` takes 2 arguments.

- [ ] **Step 3: Implement.**

```rust
pub struct Server<L, E = Hyper, W = Plain, S = Pending<()>> {
    listener: L,
    settings: Settings,
    tasks: W,
    signal: S,
    engine: PhantomData<fn() -> E>,
}
```

`Server::new` sets `signal: std::future::pending()`. `engine` and `task_wrap` carry `signal` through. Add:

```rust
/// Stops accepting once `signal` resolves, lets in-flight requests finish within the drain
/// timeout and aborts the rest. Without it, `serve` runs until the listener fails.
pub fn graceful_shutdown<T: Future<Output = ()> + Send>(self, signal: T) -> Server<L, E, W, T> {
    Server {
        listener: self.listener,
        settings: self.settings,
        tasks: self.tasks,
        signal,
        engine: PhantomData,
    }
}
```

Move the existing `impl` block's bound to `impl<L: Listener, E: Engine, W: TaskWrap, S: Future<Output = ()> + Send> Server<L, E, W, S>`, keep `config`/`settings`/`engine`/`task_wrap`/`local_addr` there, and change `serve` to `pub async fn serve<R: Routes>(self, routes: R)`, destructuring `signal` from `self` and pinning it where `shutdown` was pinned. Body unchanged otherwise.

- [ ] **Step 4: Update callers.** `testing::spawn`: `tokio::spawn(server.graceful_shutdown(async { let _ = stopped.await; }).serve(routes()))`. `hello.rs`: `server.graceful_shutdown(async { ... }).serve(routes).await`. `serve-fast-http.rs`: `server.serve(apps::fast_http()).await`. README usage and HTTP-DESIGN §8 show `.graceful_shutdown(signal)` as optional.

- [ ] **Step 5: Run the gate**

Run: `just test && just check && just clippy`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
mise exec -- git commit -am "feat(server)!: make graceful shutdown an optional builder step"
```

---

### Task 6: Engine allocation bench

Gives an in-process, network-free count of allocations per request through hyper, so Tasks 7–9 and 12 can be judged without a load run.

**Files:**
- Create: `crates/server/benches/engine.rs`
- Modify: `crates/server/Cargo.toml` (dev-deps `criterion`, `fast-http-bench-support`; `[[bench]] name = "engine" harness = false`)
- Modify: `justfile` (bench target list in the `bench` recipe doc)

**Interfaces:**
- Consumes: `Server::serve(routes)` (Task 5), `Memory`/`MemoryConnector`.
- Produces: bench ids `engine/h1/plaintext`, `engine/h1/json` printing `alloc engine/h1/<case>: count=… bytes=…`.

- [ ] **Step 1: Write the bench.** A current-thread runtime runs both the server and a raw-bytes client, so every allocation of the request is on the measured thread; the client reuses one buffer and allocates nothing per request:

```rust
//! Allocations and time per HTTP/1.1 request through the hyper engine, over an in-memory
//! connection, so engine costs show without a network.

use criterion::{Criterion, criterion_group, criterion_main};
use fast_http_bench_support::measure;
use fast_http_core::{Json, Path};
use fast_http_router::{Route, Router};
use fast_http_server::Server;
use fast_http_transport::Memory;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, DuplexStream},
    runtime::{Builder, Runtime},
};

const CASES: [(&str, &[u8]); 2] = [
    ("plaintext", b"GET /health HTTP/1.1\r\nhost: bench\r\n\r\n"),
    ("json", b"GET /users/42 HTTP/1.1\r\nhost: bench\r\n\r\n"),
];

/// One request on a kept-alive connection, read until the response body is complete.
async fn round_trip(stream: &mut DuplexStream, request: &[u8], buf: &mut [u8]) -> usize {
    let mut read = 0;

    stream.write_all(request).await.expect("write");

    loop {
        read += stream.read(&mut buf[read..]).await.expect("read");

        if let Some(end) = complete(&buf[..read]) {
            return end;
        }
    }
}

/// Length of a complete response with a `content-length` header in `bytes`, if there is one.
fn complete(bytes: &[u8]) -> Option<usize> {
    let head = bytes.windows(4).position(|window| window == b"\r\n\r\n")? + 4;
    let text = std::str::from_utf8(&bytes[..head]).ok()?;
    let length = text
        .lines()
        .find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length: ").map(str::to_owned))?
        .parse::<usize>()
        .ok()?;

    (bytes.len() >= head + length).then_some(head + length)
}

async fn user(Path(id): Path<u64>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "id": id, "name": "Ada" }))
}

fn connect(runtime: &Runtime) -> DuplexStream {
    runtime.block_on(async {
        let (memory, connector) = Memory::new(64 * 1024);
        let routes = Router::new()
            .route(Route::new("/health").expect("pattern").get(async || "ok"))
            .route(Route::new("/users/{id}").expect("pattern").get(user));
        let server = Server::bind(&memory, ()).await.expect("bind");

        tokio::spawn(server.serve(routes));

        connector.connect().await.expect("connect")
    })
}

fn engine(c: &mut Criterion) {
    let runtime = Builder::new_current_thread().enable_all().build().expect("runtime");
    let mut stream = connect(&runtime);
    let mut buf = vec![0_u8; 16 * 1024];

    for (name, request) in CASES {
        let id = format!("engine/h1/{name}");
        let info = measure(
            || (),
            |()| runtime.block_on(round_trip(&mut stream, request, &mut buf)),
        );

        println!("alloc {id}: count={} bytes={}", info.count_total, info.bytes_total);
        c.bench_function(&id, |b| {
            b.iter(|| runtime.block_on(round_trip(&mut stream, request, &mut buf)));
        });
    }
}

criterion_group!(benches, engine);
criterion_main!(benches);
```

Match the handler and extractor names to the real fast-http API used in `crates/server/examples/hello.rs` (`Json` lives in `fast-http-format`; use whatever the example uses and add that crate with the `json` feature to dev-deps if needed). Add `serde_json` to the server crate's dev-deps only if it is already a workspace dep; otherwise return a fixed `&'static str` JSON body. `allocation_counter::measure` counts the current thread only, which is why the runtime is current-thread.

- [ ] **Step 2: Compile only**

Run: `cargo bench -p fast-http-server --bench engine --no-run`
Expected: builds.

- [ ] **Step 3: Ask the user** to approve `just bench engine -- --warm-up-time 1 --measurement-time 2`; record the baseline `count`/`bytes` lines in the commit message.

- [ ] **Step 4: Commit**

```bash
mise exec -- git commit -am "bench(server): count allocations per request through the hyper engine"
```

---

### Task 7: Find and fix the ~9 KB per request

**Files:**
- Investigate: `crates/server/src/engine/hyper/mod.rs`, `crates/core/src/endpoint/dispatch.rs`, `crates/router/src/router/mod.rs`
- Modify: whichever holds the request head too long (expected: `engine/hyper/mod.rs`)

**Interfaces:**
- Consumes: the `engine` bench (Task 6).

Background: hyper's h1 reader (`hyper-1.12.0/src/proto/h1/io.rs:231`) reserves 8 KB (`INIT_BUFFER_SIZE`) whenever the read buffer has less room. `BytesMut::reserve` reclaims the old allocation only when no `Bytes` split off it (`role.rs:225`, the request head; header values point into it) are alive. If anything keeps a header value or the head alive until the next read, every request allocates a fresh 8 KB.

- [ ] **Step 1: Confirm it is the read buffer.** Temporarily add to the bench a counting allocator wrapper that prints sizes ≥ 4096 (or run the engine bench under `just profile`-style `sample` and search for `BytesMut::reserve`). Expected: one ~8 KB allocation per request from `poll_read_from_io`. If it is something else, record what it is and adjust the following steps to that site.

- [ ] **Step 2: Find what holds the head.** Check, in order:
  1. hyper's own dispatcher: does it keep the previous request's `Parts` until the response is written? Read `hyper-1.12.0/src/proto/h1/dispatch.rs` (`poll_read_head`, `Server::recv_msg`). If hyper itself keeps it, the fix must be on hyper's side; then stop, report to the user and skip to Step 5 with "not fixable without the custom engine".
  2. Our service future (`engine/hyper/mod.rs` `service_fn`): the response future captures `req` until completion; the response body (`Full<Bytes>`) is written after the future completes, so the head should already be dropped — verify.
  3. Response headers built from request header values (e.g. echoing `content-type`) keep the buffer alive until the response is flushed.

- [ ] **Step 3: Write the guard.** Change the bench's println into a budget once the fix is known: `assert!(info.bytes_total < 4096, "engine/h1/{name} reallocated the read buffer")`.

- [ ] **Step 4: Fix and measure.** Apply the fix at the site found in Step 2 and re-run the engine bench (approved command from Task 6). Keep only if bytes per request drop below 4 KB.

- [ ] **Step 5: Commit or record.** Commit with before/after numbers (`fix(server): reuse hyper's read buffer across requests`), or, if the cause is inside hyper, add the finding to the spec under "Findings" and commit that (`docs: record hyper read-buffer finding`).

---

### Task 8: One routes clone per connection

**Files:**
- Modify: `crates/server/src/engine/hyper/mod.rs`

- [ ] **Step 1: Baseline** the `engine` bench numbers from Task 7.

- [ ] **Step 2: Implement.** Replace `service_fn` with a named service so the per-request future borrows nothing it must clone:

```rust
/// The per-connection hyper service; clones nothing per request but the `Arc` it already holds.
struct Service<R> {
    routes: Arc<R>,
    peer: Option<PeerAddr>,
    max_body_bytes: usize,
}

impl<R: Routes> hyper::service::Service<http::Request<Incoming>> for Service<R> {
    type Response = Response;
    type Error = Infallible;
    type Future = Dispatch<R>;

    fn call(&self, req: http::Request<Incoming>) -> Self::Future {
        // ...
    }
}
```

hyper requires `'static` futures, so `Dispatch<R>` must own an `Arc<R>`; the gain is only real if the closure currently clones twice (closure capture + async block). Measure: if the engine bench shows no time or allocation change, revert and record "no gain" in the plan's progress notes instead of committing.

- [ ] **Step 3: Run tests** `just test`, then the engine bench (approved), compare.

- [ ] **Step 4: Commit if kept**

```bash
mise exec -- git commit -am "perf(server): build the hyper service once per connection"
```

---

### Task 9: `PeerAddr` without extension allocations

**Files:**
- Create: `crates/core/src/extract/peer.rs`, `crates/core/src/extract/peer/tests.rs`
- Modify: `crates/core/src/extract/extension.rs` (remove `PeerAddr` there), `crates/core/src/extract/mod.rs`, `crates/core/src/lib.rs`
- Modify: `crates/server/src/engine/hyper/mod.rs`, `crates/server/src/config/mod.rs` (doc of `PEER_ADDR`)

**Interfaces:**
- Produces: `pub struct PeerAddr(pub SocketAddr)` (unchanged API); `pub fn with_peer<F: Future>(peer: SocketAddr, future: F) -> WithPeer<F>`; `PeerAddr::from_parts` reads the extension first, then the scope.

The value lives in a thread-local that `WithPeer` sets around each poll of the request future and restores afterwards. Extractors run inside that poll, so they see it on any runtime thread, without tokio in core.

- [ ] **Step 1: Write the failing tests** in `peer/tests.rs`:

```rust
use std::{net::SocketAddr, pin::pin, task::{Context, Poll, Waker}};

use http::Request;

use super::*;

fn poll<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("test future suspended"),
    }
}

#[test]
fn peer_is_read_inside_the_scope_only() {
    let addr: SocketAddr = "10.0.0.1:4000".parse().expect("addr");
    let (parts, ()) = Request::new(()).into_parts();

    assert_eq!(poll(with_peer(addr, async { PeerAddr::from_parts(&parts) })), Ok(PeerAddr(addr)));
    assert!(PeerAddr::from_parts(&parts).is_err());
}

#[test]
fn extension_wins_over_scope() {
    let scoped: SocketAddr = "10.0.0.1:4000".parse().expect("addr");
    let inserted: SocketAddr = "10.0.0.2:5000".parse().expect("addr");
    let mut request = Request::new(());

    request.extensions_mut().insert(PeerAddr(inserted));

    let (parts, ()) = request.into_parts();

    assert_eq!(poll(with_peer(scoped, async { PeerAddr::from_parts(&parts) })), Ok(PeerAddr(inserted)));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo nextest run -p fast-http-core peer`
Expected: compile error, `with_peer` missing.

- [ ] **Step 3: Implement** `peer.rs`:

```rust
//! The connected peer's address, carried around each poll of a request instead of in the
//! request's extensions, which would allocate per request.

use std::{
    any::type_name,
    cell::Cell,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use http::request::Parts;
use pin_project_lite::pin_project;

use crate::{FromParts, Rejection};

thread_local! {
    static PEER: Cell<Option<SocketAddr>> = const { Cell::new(None) };
}

/// Address of the connected peer, set by the engine with [`with_peer`] or inserted as an extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerAddr(pub SocketAddr);

impl FromParts for PeerAddr {
    type Rejection = Rejection;

    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<PeerAddr>()
            .copied()
            .or_else(|| PEER.get().map(PeerAddr))
            .ok_or(Rejection::MissingExtension {
                name: type_name::<PeerAddr>(),
            })
    }
}

pin_project! {
    /// A request future that sees `peer` as its [`PeerAddr`] while it is polled.
    pub struct WithPeer<F> {
        peer: SocketAddr,
        #[pin]
        future: F,
    }
}

/// Runs `future` with `peer` as the [`PeerAddr`] its extractors read.
pub fn with_peer<F: Future>(peer: SocketAddr, future: F) -> WithPeer<F> {
    WithPeer { peer, future }
}

impl<F: Future> Future for WithPeer<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = self.project();
        let previous = PEER.replace(Some(*this.peer));
        let restore = Restore(previous);
        let output = this.future.poll(cx);

        drop(restore);

        output
    }
}

/// Puts the outer peer back even when the inner poll panics.
struct Restore(Option<SocketAddr>);

impl Drop for Restore {
    fn drop(&mut self) {
        PEER.set(self.0);
    }
}
```

Export `with_peer`, `WithPeer` and `PeerAddr` from `extract/mod.rs` and `lib.rs`. In the hyper engine replace the extension insert with:

```rust
let dispatch = routes.dispatch(req.map(|body| Limited::new(body, max_body_bytes)));

match peer {
    Some(PeerAddr(addr)) => Either::Left(with_peer(addr, dispatch)),
    None => Either::Right(dispatch),
}
```

(use `futures_util::future::Either` only if it is already in the server crate's tree; otherwise wrap with `with_peer` always and pass an `Option<SocketAddr>` field, changing `WithPeer::peer` to `Option<SocketAddr>`). Update the `PEER_ADDR` doc: it no longer costs allocations.

- [ ] **Step 4: Run tests and the engine bench** (approved): `just test`; expect two fewer allocations per request with `peer_addr` on.

- [ ] **Step 5: Commit**

```bash
mise exec -- git commit -am "perf: carry PeerAddr per poll instead of in request extensions"
```

---

### Task 10: Smaller dispatch futures

**Files:**
- Create: `benchmarks/comparison/tests/sizes.rs`
- Modify: whichever of `crates/router/src/router/mod.rs` (`Matcher::run`), `crates/router/src/route/list/*.rs`, `crates/core/src/endpoint/dispatch.rs` hold the largest state.

- [ ] **Step 1: Write the size test** (it is the guard; set the bound to today's size first so it passes, then lower it):

```rust
//! Upper bounds on the dispatch future of the comparison API; large futures cost memmoves on
//! every poll.

use std::mem::size_of_val;

use fast_http_bench::{apps, cases::CASES};
use fast_http_core::Routes;

#[test]
fn dispatch_future_stays_small() {
    let app = apps::fast_http();

    for case in CASES {
        let size = size_of_val(&app.dispatch(case.request()));

        println!("{}: {size} bytes", case.name);
        assert!(size <= 2_048, "{} dispatch future is {size} bytes", case.name);
    }
}
```

Run: `cargo nextest run --manifest-path benchmarks/Cargo.toml -p fast-http-bench dispatch_future_stays_small --no-capture` and note the sizes.

- [ ] **Step 2: Find the large states.** Build with `RUSTFLAGS=-Zprint-type-sizes` is nightly-only; instead add temporary `println!("{}", size_of_val(&fut))` probes at `Matcher::run`, `RouteList::call`, `EndpointSet::call` and the handler call to see which level grows. Known candidates: `ParamBuf` (264 B) held across `.await` in `Matcher::run`; the 16-way `match` in `EndpointSet::call` makes the future the size of the largest endpoint future plus the parts copy.

- [ ] **Step 3: Shrink.** Techniques, in order, each followed by the size test and `just bench stack` (approved):
  1. Build `PathParams` before the first `.await` and drop `buf`'s unused tail by storing only `&[&str]` of the used length (copy into a smaller fixed array sized by the route's `names().len()` is not possible statically; instead keep `ParamBuf` but move the route call into a non-async fn returning `impl Future` so `buf` is not part of the outer state machine across the await).
  2. Make `EndpointSet::call` return the endpoint's future directly through a generated enum (`Either`-like per arity) instead of an `async fn` that awaits inside a `match`, which removes one nested state layer.
  3. Remove the `parts` copy in `Matcher::run` when not layered by passing `&parts` into a future that owns `parts` (one state) instead of two.

- [ ] **Step 4: Lower the bound** in the test to the new size rounded up to the next 128 bytes.

- [ ] **Step 5: Commit** with before/after sizes and stack-bench times:

```bash
mise exec -- git commit -am "perf(router): shrink the dispatch future"
```

---

### Task 11: Segment-trie route lookup

**Files:**
- Create: `crates/router/src/router/trie.rs`, `crates/router/src/router/trie/tests.rs`
- Delete: `crates/router/src/router/index.rs`
- Modify: `crates/router/src/router/mod.rs`

**Interfaces:**
- Produces: `pub(super) struct Trie`; `Trie::insert(&mut self, segments: &[Segment], index: usize)` (panics past `u32::MAX`, like `Index::push`); `Trie::find(&self, path: &str) -> Option<usize>`, the lowest-indexed route whose shape matches. `Router::lookup` then fills `buf` with `pattern(index).matches(path, buf)`.

Rule kept from today: routes are tried in declaration order, so the lowest matching index wins, whatever its specificity.

- [ ] **Step 1: Write the failing tests** in `trie/tests.rs`:

```rust
use super::*;
use crate::Pattern;

fn trie(patterns: &[&'static str]) -> Trie {
    let mut trie = Trie::default();

    for (index, pattern) in patterns.iter().enumerate() {
        trie.insert(Pattern::parse(pattern).expect("pattern").segments(), index);
    }

    trie
}

#[test]
fn lowest_declared_match_wins() {
    let trie = trie(&["/users/{id}", "/users/me", "/**"]);

    assert_eq!(trie.find("/users/me"), Some(0));
    assert_eq!(trie.find("/users/7"), Some(0));
    assert_eq!(trie.find("/other/x"), Some(2));
}

#[test]
fn literal_and_param_branches_are_both_tried() {
    let trie = trie(&["/a/b/c", "/a/{x}/d"]);

    assert_eq!(trie.find("/a/b/d"), Some(1));
    assert_eq!(trie.find("/a/b/c"), Some(0));
    assert_eq!(trie.find("/a/b"), None);
}

#[test]
fn rest_needs_at_least_one_segment_and_root_matches_only_root() {
    let trie = trie(&["/", "/files/{**path}"]);

    assert_eq!(trie.find("/"), Some(0));
    assert_eq!(trie.find("/files"), None);
    assert_eq!(trie.find("/files/a/b"), Some(1));
}

#[test]
fn empty_segments_never_match_parameters() {
    let trie = trie(&["/users/{id}", "/users/*/x"]);

    assert_eq!(trie.find("/users/"), None);
    assert_eq!(trie.find("/users//x"), None);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo nextest run -p fast-http-router trie`
Expected: compile error, `Trie` missing.

- [ ] **Step 3: Implement** `trie.rs`:

```rust
//! Route lookup by path segment: literal children, one child for single-segment parameters and
//! wildcards, and the routes that end or take the rest of the path at each node.

use crate::Segment;

/// One node per pattern prefix; route indices are kept as `u32`.
#[derive(Debug, Default)]
pub(super) struct Trie {
    literal: Vec<(&'static str, Trie)>,
    single: Option<Box<Trie>>,
    end: Option<u32>,
    rest: Option<u32>,
}

impl Trie {
    /// Adds the route at `index` with `segments`.
    ///
    /// # Panics
    ///
    /// When `index` does not fit in a `u32`.
    pub(super) fn insert(&mut self, segments: &[Segment], index: usize) {
        let index = u32::try_from(index).expect("a router holds at most u32::MAX routes");
        let mut node = self;

        for segment in segments {
            node = match segment {
                Segment::Literal(text) => {
                    let position = match node.literal.binary_search_by_key(text, |entry| entry.0) {
                        Ok(position) => position,
                        Err(position) => {
                            node.literal.insert(position, (text, Trie::default()));

                            position
                        }
                    };

                    &mut node.literal[position].1
                }
                Segment::Param(_) | Segment::Any => node.single.get_or_insert_with(Box::default),
                Segment::Rest(_) => {
                    node.rest.get_or_insert(index);

                    return;
                }
            };
        }

        node.end.get_or_insert(index);
    }

    /// The lowest-indexed route whose segments match `path`.
    pub(super) fn find(&self, path: &str) -> Option<usize> {
        let rest = path.strip_prefix('/')?;

        if rest.is_empty() {
            return self.end.map(|index| index as usize);
        }

        self.walk(rest).map(|index| index as usize)
    }

    fn walk(&self, rest: &str) -> Option<u32> {
        let (head, tail) = match rest.split_once('/') {
            Some((head, tail)) => (head, Some(tail)),
            None => (rest, None),
        };
        let here = self.rest.filter(|_| !rest.is_empty());
        let literal = self
            .literal
            .binary_search_by_key(&head, |entry| entry.0)
            .ok()
            .and_then(|position| self.literal[position].1.next(tail));
        let single = self
            .single
            .as_deref()
            .filter(|_| !head.is_empty())
            .and_then(|child| child.next(tail));

        [here, literal, single].into_iter().flatten().min()
    }

    fn next(&self, tail: Option<&str>) -> Option<u32> {
        match tail {
            None => self.end,
            Some(tail) => self.walk(tail),
        }
    }
}

#[cfg(test)]
mod tests;
```

Note on `Rest`: `get_or_insert` keeps the first (lowest) index, since routes are inserted in increasing index order; same for `end`.

- [ ] **Step 4: Wire into the router.** In `router/mod.rs` replace `index: Index` with `trie: Trie`, `index.push(route.pattern().first_literal(), L::LEN)` with `trie.insert(route.pattern().segments(), L::LEN)` (and the same in `merge`), and `lookup` with:

```rust
pub fn lookup<'p>(&self, path: &'p str, buf: &mut ParamBuf<'p>) -> Option<usize> {
    let index = self.trie.find(path)?;

    self.routes.pattern(index).matches(path, buf).then_some(index)
}
```

Delete `index.rs`. If `Pattern::first_literal` has no other user, remove it.

- [ ] **Step 5: Run all router tests**

Run: `cargo nextest run -p fast-http-router`
Expected: every existing router test passes unchanged, plus the new trie tests.

- [ ] **Step 6: Measure** (approved): `just bench router routing -- --warm-up-time 1 --measurement-time 2`. Keep only if lookups are not slower than the first-literal index for small tables and faster for the large/growth cases.

- [ ] **Step 7: Commit**

```bash
mise exec -- git commit -am "perf(router): look routes up in a segment trie"
```

---

### Task 12: 405 `Allow` as a method set

**Files:**
- Create: `crates/core/src/rejection/methods.rs`, `crates/core/src/rejection/methods/tests.rs`
- Modify: `crates/core/src/rejection/mod.rs`, `crates/core/src/rejection/tests.rs`
- Modify: `crates/core/src/endpoint/set.rs`
- Modify: any user of `Rejection::MethodNotAllowed { allow }` (`grep -rn "MethodNotAllowed" crates benchmarks --include=*.rs`)

**Interfaces:**
- Produces: `pub struct Methods(u16)` with `const fn empty()`, `fn insert(&mut self, method: &Method) -> bool` (false for extension methods), `fn contains(&self, method: &Method) -> bool`, `fn iter(&self) -> impl Iterator<Item = Method>`, and `Display` rendering `GET, HEAD, POST` in the standard order; `Rejection::MethodNotAllowed { allow: Methods }`.

Endpoints with extension (non-standard) methods are rare; they are left out of `Allow` and a `debug!` notes it (the response stays a valid 405).

- [ ] **Step 1: Write the failing tests** in `methods/tests.rs`:

```rust
use http::Method;

use super::*;

#[test]
fn standard_methods_render_in_order_without_duplicates() {
    let mut methods = Methods::empty();

    assert!(methods.insert(&Method::POST));
    assert!(methods.insert(&Method::GET));
    assert!(methods.insert(&Method::GET));

    assert_eq!(methods.to_string(), "GET, POST");
    assert!(methods.contains(&Method::GET));
    assert!(!methods.contains(&Method::PUT));
}

#[test]
fn extension_methods_are_not_stored() {
    let mut methods = Methods::empty();

    assert!(!methods.insert(&Method::from_bytes(b"PURGE").expect("method")));
    assert_eq!(methods.to_string(), "");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo nextest run -p fast-http-core methods`
Expected: compile error.

- [ ] **Step 3: Implement** `methods.rs`:

```rust
//! A set of the standard HTTP methods in one integer, for 405 `Allow` headers without allocating.

use std::fmt;

use http::Method;

/// The standard methods in `Allow` order; a method's bit is its position here.
const STANDARD: [Method; 9] = [
    Method::GET,
    Method::HEAD,
    Method::POST,
    Method::PUT,
    Method::DELETE,
    Method::PATCH,
    Method::OPTIONS,
    Method::CONNECT,
    Method::TRACE,
];

/// Set of standard HTTP methods.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Methods(u16);

impl Methods {
    /// No methods.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Adds `method`; returns false for extension methods, which the set cannot hold.
    pub fn insert(&mut self, method: &Method) -> bool {
        let Some(bit) = bit(method) else {
            return false;
        };

        self.0 |= bit;

        true
    }

    /// Whether `method` is in the set.
    pub fn contains(&self, method: &Method) -> bool {
        bit(method).is_some_and(|bit| self.0 & bit != 0)
    }

    /// The methods in `Allow` order.
    pub fn iter(&self) -> impl Iterator<Item = Method> + '_ {
        STANDARD
            .iter()
            .enumerate()
            .filter(|(position, _)| self.0 & (1 << position) != 0)
            .map(|(_, method)| method.clone())
    }
}

impl fmt::Display for Methods {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (position, method) in self.iter().enumerate() {
            if position > 0 {
                f.write_str(", ")?;
            }

            f.write_str(method.as_str())?;
        }

        Ok(())
    }
}

fn bit(method: &Method) -> Option<u16> {
    STANDARD
        .iter()
        .position(|standard| standard == method)
        .map(|position| 1 << position)
}
```

`Method` constants are `const`, so the `STANDARD` array is valid; `Method::clone` of a standard method does not allocate.

- [ ] **Step 4: Use it.** In `rejection/mod.rs` declare `mod methods; pub use methods::Methods;`, change the field to `allow: Methods`, and in `IntoResponse` replace the `Vec` join with `HeaderValue::from_str(&allow.to_string())` (one allocation on the 405 path, down from eight). Export `Methods` from `lib.rs`. In `endpoint/set.rs` replace `collect_allow` with:

```rust
fn collect_allow(methods: &[&Method]) -> Methods {
    let mut allow = Methods::empty();

    for method in methods {
        if !allow.insert(method) {
            tracing::debug!(target: "fast_http::core", %method, "extension method left out of Allow");
        }
    }

    if allow.contains(&Method::GET) {
        allow.insert(&Method::HEAD);
    }

    allow
}
```

and the empty set's `Vec::new()` with `Methods::empty()`. Update `rejection/tests.rs` (`allow: vec![Method::GET, Method::POST]` → a `Methods` with both inserted) and any other users. Use the crate's existing tracing target constant if it has one.

- [ ] **Step 5: Run tests**

Run: `just test`
Expected: pass; the 405 response tests still produce `Allow: GET, HEAD, POST`-style headers.

- [ ] **Step 6: Measure** the 405 case in `just bench core` (approved); expect at most 1 allocation for the selection plus the response.

- [ ] **Step 7: Commit**

```bash
mise exec -- git commit -am "perf(core)!: represent 405 Allow methods as a bitset"
```

---

### Task 13: hyper options

**Files:**
- Modify: `crates/server/src/config/mod.rs` (`ServerConfig`, `Settings`)
- Modify: `crates/server/src/config/tests.rs` if present
- Modify: `crates/server/src/engine/hyper/mod.rs`
- Modify: `crates/server/README.md`

**Interfaces:**
- Produces: `ServerConfig::PROTOCOLS: Protocols` and `Settings::protocols`, with `#[non_exhaustive] pub enum Protocols { Auto, Http1, Http2 }` (default `Auto`); `ServerConfig::H1_WRITEV: Option<bool>` (`None` = hyper's adaptive default) and `ServerConfig::H1_PIPELINE_FLUSH: bool` (default false).

- [ ] **Step 1: Write the failing test** in the server tests (`server/tests.rs`):

```rust
#[tokio::test]
async fn http1_only_server_rejects_the_h2_preface_and_serves_h1() {
    let (connector, running) = testing::start(|settings| settings.protocols = Protocols::Http1).await;
    let mut sender = testing::h1(connector.connect().await.expect("connect")).await;
    let response = sender
        .send_request(testing::request(Method::GET, "/health", b""))
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let h2 = testing::h2(connector.connect().await.expect("connect")).await;

    assert!(h2.ready().await.is_err() || h2.is_closed());

    running.stop().await;
}
```

Adapt the h2 assertion to how the existing h2 tests observe a failed handshake (`testing::h2` panics on handshake failure today; add a non-panicking variant `try_h2` returning `Result` if needed).

- [ ] **Step 2: Run to verify failure**

Run: `cargo nextest run -p fast-http-server http1_only`
Expected: compile error, `Protocols` missing.

- [ ] **Step 3: Implement.** `HyperState` keeps `builder: auto::Builder<Spawner<W>>` and adds `protocols: Protocols`. In `Engine::new` apply:

```rust
let h1 = builder.http1();

h1.timer(TokioTimer::new())
    .header_read_timeout(settings.header_read_timeout)
    .max_buf_size(settings.max_header_bytes.max(MIN_H1_BUFFER))
    .max_headers(settings.max_headers)
    .pipeline_flush(settings.h1_pipeline_flush);

if let Some(writev) = settings.h1_writev {
    h1.writev(writev);
}

match settings.protocols {
    Protocols::Auto => {}
    Protocols::Http1 => builder = builder.http1_only(),
    Protocols::Http2 => builder = builder.http2_only(),
}
```

Check the exact `auto::Builder::http1_only`/`http2_only` signatures in `hyper-util-0.1.21/src/server/conn/auto/mod.rs` (they take `self` and return `Self`), and order the calls so the http1/http2 settings are applied before or after as that API requires. Document each new constant on `ServerConfig` and in the README feature list.

- [ ] **Step 4: Run tests** `just test`.

- [ ] **Step 5: Measure** (approved) with the engine bench and a short load run per option: `writev` `Some(true)`/`Some(false)`/`None`, `pipeline_flush` on/off, `Protocols::Http1` vs `Auto`, and `serve_connection` vs `serve_connection_with_upgrades` (temporary local edit). Change a default only when the gain is outside the run-to-run spread of Task 2's repeats; record the numbers in the commit message.

- [ ] **Step 6: Commit**

```bash
mise exec -- git commit -am "feat(server): expose protocol selection and hyper h1 write options"
```

---

### Task 14: Final comparison

- [ ] **Step 1: Ask the user** to approve `just load-linux 0-3 4-7 --repeat 3` (or the macOS `just load --repeat 3` if Docker is unavailable).
- [ ] **Step 2: Check the spec's success criteria**: allocations per request below axum's and actix's in every scenario; bytes per request under 1 KB for `plaintext` and `json` (only if Task 7 found a fix); median CPU µs per request at least 20% below axum with non-overlapping ranges; no regression in `just bench router routing stack core`.
- [ ] **Step 3: Report** the table to the user, and list which criteria are not met and why (for example a hyper-internal cost only a custom engine can remove). Record the results path in the spec under "Findings".
