# fast-http-load

## Role

Network comparison of fast-http, axum and actix-web: latency, throughput, server memory over time under idle, light, medium and heavy load, and allocations per request. Never published.

## Usage

Needs `oha` on `PATH` (`cargo install oha`).

```sh
just load                                                  # full run, several minutes
just load --duration 2 --idle 2 --warmup 1 --profiles light # smoke run
just load --frameworks fast-http,axum --scenarios json --workers 4
```

`just load` builds the release binaries and writes `benchmarks/load/results/<unix time>/` (gitignored):

- `summary.json`: the configuration and every measurement.
- `summary.md`: latency/throughput, memory and allocation tables (also printed).
- `rss.csv`: `framework,phase,at_ms,rss_bytes`, one row per memory sample, for plotting memory over time.

Any unknown or malformed flag prints the full option list.

## Internal role

For each framework, `load` spawns `serve-<framework>`, which binds `127.0.0.1:0` and prints `listening <app addr> <control addr>`. A sampler thread records the server's RSS (sysinfo) every 100 ms for the whole run. Phases, in order:

1. Idle (`--idle`).
2. Warmup per scenario: unmeasured, unthrottled load.
3. Allocation pass per scenario: `--alloc-requests` requests over one connection while the server's counting global allocator is enabled through the control socket; reported as allocations and bytes per request.
4. Each profile × scenario for `--duration`: light and medium at a fixed rate (`oha -q` with latency correction, so latency is not hidden by coordinated omission), heavy unthrottled.
5. Cooldown idle, which shows whether memory is returned after load.

Scenarios: `plaintext` (`GET /health`), `json` (`GET /users/42`), `create` (`POST /users` with a JSON body).

Fairness notes:

- Every server is served over HTTP/1.1 only, with `--workers` threads (default half the cores, leaving the rest for `oha` on the same machine). actix uses its usual thread-per-core runtimes; fast-http and axum a multi-threaded tokio runtime.
- The allocation counter is off outside the allocation passes, so its atomics do not cost the throughput phases.
- Loopback numbers measure framework overhead, not a deployment; compare runs from the same machine only.

## Feature flags

None.
