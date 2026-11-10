# fast-http-load

## Role

Network comparison of fast-http, axum and actix-web: latency, throughput, server memory over time under idle, light, medium and heavy load, and allocations per request. Never published.

## Usage

Needs `oha` on `PATH` (`cargo install oha`).

```sh
just load                                                  # full run, several minutes
just load --duration 2 --idle 2 --warmup 1 --profiles light # smoke run
just load --frameworks fast-http,axum --scenarios json --workers 4
just load --repeat 3                                       # three runs, rotated framework order
```

`just load` builds the release binaries and writes `benchmarks/load/results/<unix time>/` (gitignored):

- `summary.json`: the configuration and every measurement of every repeat, unaggregated.
- `summary.md`: latency/throughput (with `cpu µs/req`, the server's CPU time per response over the load window), memory and allocation tables (also printed). With `--repeat N` every number is the median over the repeats, and `req/s` and `cpu µs/req` also show `(min–max)`. `cpu µs/req` is the primary metric: it is the CPU the server spends per response, so it barely depends on how busy `oha` and the machine are, unlike `req/s`.
- `rss.csv`: `framework,repeat,phase,at_ms,rss_bytes,cpu_ms`, one row per memory sample, for plotting memory and accumulated server CPU time over time.

`--repeat N` runs the whole matrix N times; repeat `i` starts with the `i`-th framework (rotated), so no framework always runs first or last and thermal or background drift averages out.

Any unknown or malformed flag prints the full option list.

### Pinned Linux runs

`--server-cpus LIST` pins every server thread (`serve-* --cpus LIST`, via `sched_setaffinity`; actix pins inside its per-worker app factory) and `--client-cpus LIST` runs `oha` under `taskset -c LIST`. Lists are `0,2` or `0-3`. Giving server and client disjoint cores removes their competition for CPU, which is the largest source of noise on a shared machine. Both flags work together with `--repeat` and are rejected on other operating systems ("CPU pinning needs Linux"). `libc` is a Linux-only dependency of this crate for this.

```sh
just load-linux 0-3 4-7 --repeat 3 --workers 4
```

`just load-linux SERVER_CPUS CLIENT_CPUS [flags]` builds `benchmarks/load/Dockerfile` (Rust, `oha`, `taskset`), mounts the repository and builds and runs inside the container; Linux artefacts live in the `fast-http-load-target` volume, not in the host `benchmarks/target`. Results land in `benchmarks/load/results`. Docker on macOS runs in a VM, so give it at least as many CPUs as the two lists use.

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
