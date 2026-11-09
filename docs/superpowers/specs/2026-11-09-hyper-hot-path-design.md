# Hot path on hyper: measurement, API cleanup and lower-layer fixes

Date: 2026-11-09. Status: draft for review.

## Goal

Make fast-http clearly cheaper per request than axum and actix-web while keeping hyper as the engine, so Upwell can add DI on top and still beat plain axum. "Cheaper" is proven with server CPU time and allocations per request, not loopback requests/s alone.

## Non-goals

- A custom HTTP/1.1 engine (httparse, chosen for later) and the thread-per-core `Rc` mode. Both get their own spec once hyper is exhausted.
- Changing the request contract: handlers keep receiving `http::Request`.
- HTTP/2 tuning beyond what falls out of the shared changes.

## 1. Measurement we can trust

The heavy-load reruns showed ±45% swings for the same server on one Mac (scheduling across performance/efficiency cores, clock changes, oha on the same CPUs). Changes:

- **CPU time per request.** The RSS sampler also records the server's cumulative CPU time (sysinfo `Process::accumulated_cpu_time`). Each phase reports `cpu µs/request = Δcpu / responses`. This metric hardly depends on how much CPU oha takes and becomes the primary comparison.
- **Repeats with rotated order.** `--repeat N` (default 1) runs the whole matrix N times, rotating which framework goes first. The report shows the median and min–max per cell; `rss.csv` gains a `repeat` column.
- **Pinned Linux run.** A `just load-linux` recipe runs the harness in a Docker container with `--cpuset-cpus` splitting server and oha cores. The server binaries take `--cpus LIST` and pin their worker threads on Linux (`sched_setaffinity` through `libc`, Linux-only and in the benchmarks workspace only). macOS has no pinning; there the repeats are the mitigation. Adding `libc` to the benchmarks workspace needs approval at implementation time.
- **Profiling recipe.** `just profile-server <framework> <scenario>` runs one server under oha and captures a `sample` (macOS) or `perf` (Linux) profile into `benchmarks/load/results/`. It is used to choose and verify the fixes below, not run in CI.

## 2. API cleanup

- `Server::serve(self, routes)` no longer takes a shutdown future. It returns when the listener fails or the process ends.
- `Server::graceful_shutdown(self, signal: S) -> Server<L, E, W, S>` sets the signal, where `S: Future<Output = ()> + Send` and the default type is `std::future::Pending<()>`. A server without a signal pays nothing for it.
- Callers (`examples/hello.rs`, `testing.rs`, the load servers, README, HTTP-DESIGN §8) are updated.

## 3. Lower-layer fixes

Each fix is kept only if the stack/router benches or the load harness show a gain or an allocation removed; otherwise it is reverted.

1. **Head buffer reuse.** hyper splits each request head out of its read buffer and reserves 8 KB again before the next read; it reuses the old memory only when no `Bytes` from the previous head are still alive. Find what keeps them alive (the dispatch future, `Parts` held across the response, extensions) and drop it before the next read. Target: the ~9 KB per request disappears for small requests.
2. **Per-request `Arc` traffic.** The service closure clones `Arc<R>` per request. Reduce this to one clone per connection where hyper's `'static` service bound allows it.
3. **`PeerAddr` without allocations.** Inserting into `Extensions` allocates its map (two allocations). Carry the peer address in a connection-level value the extractor reads instead, keeping the `PeerAddr` extractor API.
4. **Dispatch future size.** The API dispatch future is about 1.9 KB and `memmove` took about 15% of a static dispatch. Without boxing, split large `async` blocks, keep extractor state out of the outer future, and guard sizes with a test asserting an upper bound.
5. **Segment-trie route table.** Replace the linear pattern scan with a segment trie keeping the current matching rules, so lookup reaches matchit's speed.
6. **`Segments` without `Vec<String>`.** Borrow segments from the path (zero allocations for the rest parameter).
7. **405 method set.** `Allow` becomes a fixed method bitset with an extension list, removing the 8 allocations of the 405 path.

## 4. hyper tuning

Each option is measured with the harness, not assumed:

- `http1().writev(..)`: vectored vs flattened writes for small bodies.
- `http1().pipeline_flush(true)`: batch flushes of pipelined responses.
- `http1().auto_date_header(true)` stays on (hyper caches the date per second).
- `serve_connection` instead of `serve_connection_with_upgrades` while no route uses upgrades, if the upgrade path costs measurable time.
- An `http1_only` setting that skips the auto builder's HTTP/2 preface detection, for deployments behind a proxy that speaks HTTP/1.1 upstream.

## Testing

- Existing server tests cover the shutdown API change; add one test that a server without `graceful_shutdown` serves requests.
- Allocation regressions are guarded by the existing allocation-counting benches; add a stack-bench case for a full request over hyper (in-memory duplex stream) so engine-level allocations are visible without a network.
- The future-size bound test from fix 4.
- Router tests stay as they are and must pass unchanged after the trie (fix 5).

## Success criteria

- Allocations per request below axum's and actix's in all three scenarios, and bytes per request under 1 KB for `plaintext` and `json`.
- Median CPU µs per request at least 20% below axum in the pinned Linux run, with min–max ranges not overlapping.
- No regression in the router and stack benches.
