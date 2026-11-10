# fast-http-bench-support

## Role

Shared helpers for every Criterion bench in the repository and in the `benchmarks/` workspace. Not published.

## Usage

```rust
use criterion::{Criterion, criterion_group, criterion_main};
use fast_http_bench_support::{Budget, Measurement, bench, criterion};

fn cases(c: &mut Criterion<Measurement>) {
    bench(c, "group", "case", Budget::Zero, || input(), |input| work(input));
}

criterion_group! { name = benches; config = criterion(); targets = cases }
criterion_main!(benches);
```

- `criterion()`: wall time (analysed by Criterion) plus allocation count and allocated bytes per iteration, printed to stderr after each case. `FAST_HTTP_COUNTERS=1` adds instructions, cycles and an `IPC` ratio, with no root: Linux needs `perf_event_paranoid <= 2`, Apple Silicon macOS works as is. If the counters cannot be opened (Intel Mac, VM) the bench exits non-zero with one line on stderr. Use `just bench-counters`.
- `bench(...)`: prints `alloc group/name: count=… bytes=…`, enforces the `Budget`, then times the routine.
- `measure(setup, routine)`: the `Allocations { count, bytes }` of one routine call.
- `ready(future)`: polls a future that must complete without suspending.

## Internal role

Installs `bench_metrics::CountingAllocator` as the global allocator of every bench binary, so its totals are process-wide: run benches on a quiescent process.

## Feature flags

None.
