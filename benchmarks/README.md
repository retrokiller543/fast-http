# benchmarks

A separate Cargo workspace (excluded from the root one) that holds every comparison against other frameworks. axum, matchit, actix-router, actix-web and sysinfo resolve into `benchmarks/Cargo.lock` only, so they never reach the fast-http crates' lockfile or feature unification.

- `comparison/` (`fast-http-bench`): the shared demo API built on fast-http, axum and actix-web, and the in-process Criterion benches `routing` and `stack`.
- `load/` (`fast-http-load`): network load comparison of the three servers, driven by `oha`.
- `criterion/`: Criterion reports and baselines of every `just bench` run (gitignored).

The workspace builds into its own `benchmarks/target`. Do not point both workspaces at one target directory: dep-info paths are relative to each workspace root, so cargo can treat a stale artifact as fresh. `just check`, `clippy`, `fmt`, `deny`, `bench` and `test-benchmarks` cover this workspace; `cargo deny` reads the root `deny.toml`.

## Reading the bench metrics

Every Criterion bench reports wall time (what Criterion analyses and compares against baselines) and, on stderr after each case, the median per-iteration allocation count and allocated bytes. Allocation counts are exact for the same work; they are process-wide, so keep the machine quiet while benching. `alloc group/name: count=… bytes=…` lines on stdout are the single-call counts the `Budget` is enforced on.

`just bench-instructions` also records retired instructions (and on Linux cycles and `IPC` = instructions / cycles). Instruction counts are nearly deterministic, unlike wall time or loopback req/s, so judge an optimisation on them: a change that lowers instructions but not time usually moved the bottleneck to memory or branches (check IPC). The recipe builds as your user and runs the bench binaries under `sudo` (macOS counters need root); Linux needs `perf_event_paranoid <= 2` or `CAP_PERFMON`. On macOS only one counter can be open per process: instructions by default, `FAST_HTTP_COUNTERS=cycles` for cycles.
