# benchmarks

A separate Cargo workspace (excluded from the root one) that holds every comparison against other frameworks. axum, matchit, actix-router, actix-web and sysinfo resolve into `benchmarks/Cargo.lock` only, so they never reach the fast-http crates' lockfile or feature unification.

- `comparison/` (`fast-http-bench`): the shared demo API built on fast-http, axum and actix-web, and the in-process Criterion benches `routing` and `stack`.
- `load/` (`fast-http-load`): network load comparison of the three servers, driven by `oha`.
- `criterion/`: Criterion reports and baselines of every `just bench` run (gitignored).

The workspace builds into its own `benchmarks/target`. Do not point both workspaces at one target directory: dep-info paths are relative to each workspace root, so cargo can treat a stale artifact as fresh. `just check`, `clippy`, `fmt`, `deny`, `bench` and `test-benchmarks` cover this workspace; `cargo deny` reads the root `deny.toml`.
