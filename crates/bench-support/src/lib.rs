//! Shared benchmark helpers: multi-metric Criterion measurements and ready-poll futures.
//!
//! Every bench reports wall time (the metric Criterion analyses) next to the heap allocation count
//! and allocated bytes per iteration. Opting in with `FAST_HTTP_COUNTERS=1` adds CPU hardware
//! counters: instructions, cycles and an `IPC` ratio. They need no root: Linux uses
//! `perf_event_open` (`perf_event_paranoid <= 2`), Apple Silicon macOS reads the kernel's per-process
//! counters. Instruction counts are nearly deterministic, so they are the metric to compare
//! optimisations on. Where the counters cannot be opened (Intel Macs, VMs, a denied
//! `perf_event_open`) the bench prints one line to stderr and exits non-zero. `just bench-counters`
//! runs the benches this way.
//!
//! Linking this crate installs [`bench_metrics::CountingAllocator`] as the global allocator. Its
//! totals are **process-wide**: [`measure`] and the per-iteration allocation metrics include every
//! thread, so run benches on an otherwise quiescent process.
//!
//! Typical use:
//!
//! ```ignore
//! fn cases(c: &mut Criterion<Measurement>) {
//!     bench(c, "group", "case", Budget::Zero, || input(), |input| work(input));
//! }
//!
//! criterion_group! { name = benches; config = criterion(); targets = cases }
//! criterion_main!(benches);
//! ```

use std::{
    future::Future,
    hint::black_box,
    pin::pin,
    process,
    task::{Context, Poll, Waker},
};

use bench_metrics::{
    AllocationCounter, AllocationMetric, BenchMeasurement, CountingAllocator, HardwareCounter,
    HardwareEvent, MetricReport, MetricScope,
};
use criterion::{BatchSize, Criterion, measurement::WallTime};

/// The Criterion measurement every bench uses: wall time plus secondary metrics.
pub type Measurement = BenchMeasurement;

/// Environment variable that opts in to hardware counters.
pub const COUNTERS_ENV_VAR: &str = "FAST_HTTP_COUNTERS";

#[global_allocator]
static ALLOCATOR: CountingAllocator<std::alloc::System> =
    CountingAllocator::new(std::alloc::System);

/// Allocation expectation for one benchmark case, enforced before timing starts.
#[derive(Debug, Clone, Copy)]
pub enum Budget {
    /// Report only.
    Report,
    /// The routine must not allocate at all.
    Zero,
    /// The routine may allocate at most this many times.
    AtMost(u64),
}

/// Heap activity of one routine call, counted process-wide.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Allocations {
    /// Allocations served, including reallocations.
    pub count: u64,
    /// Bytes requested by those allocations.
    pub bytes: u64,
}

/// Criterion configured with wall time, allocation metrics and, when opted in, hardware counters.
pub fn criterion() -> Criterion<Measurement> {
    let measurement = secondary_counters(
        BenchMeasurement::new(WallTime)
            .with_secondary("allocations", counter(AllocationMetric::Allocations))
            .with_secondary("allocated bytes", counter(AllocationMetric::AllocatedBytes)),
    );

    Criterion::default().with_measurement(measurement)
}

fn counter(metric: AllocationMetric) -> AllocationCounter {
    AllocationCounter::new(metric, MetricScope::Process)
        .expect("bench-support installs the counting allocator")
}

fn counters_request() -> Option<String> {
    std::env::var(COUNTERS_ENV_VAR)
        .ok()
        .filter(|value| !value.is_empty() && value != "0")
}

fn counters_unavailable(reason: impl std::fmt::Display) -> ! {
    eprintln!("{COUNTERS_ENV_VAR} requested hardware counters, but they are unavailable: {reason}");

    process::exit(2)
}

fn secondary_counters(measurement: BenchMeasurement) -> BenchMeasurement {
    if counters_request().is_none() {
        return measurement;
    }

    let open = |event| {
        HardwareCounter::new(event, MetricScope::Process)
            .unwrap_or_else(|error| counters_unavailable(format_args!("{error:#}")))
    };

    measurement
        .with_secondary("instructions", open(HardwareEvent::Instructions))
        .with_secondary("cycles", open(HardwareEvent::Cycles))
        .with_ratio("IPC", "instructions", "cycles")
}

/// Drives a future that must finish without suspending, which holds for every in-memory request.
pub fn ready<F: Future>(future: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());

    match pin!(future).poll(&mut cx) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("benchmarked future suspended; use only in-memory bodies"),
    }
}

/// Allocations made by one call of `routine`, after two warm-up calls; `setup` runs outside the count.
pub fn measure<I, O>(mut setup: impl FnMut() -> I, mut routine: impl FnMut(I) -> O) -> Allocations {
    for _ in 0..2 {
        drop(black_box(routine(setup())));
    }

    let input = setup();
    let before = bench_metrics::allocation_stats().expect("counting allocator is installed");

    drop(black_box(routine(input)));

    let after = bench_metrics::allocation_stats().expect("counting allocator is installed");

    Allocations {
        count: after.allocations - before.allocations,
        bytes: after.allocated_bytes - before.allocated_bytes,
    }
}

/// Reports allocations, enforces `budget`, then times the routine with Criterion.
///
/// `setup` builds a fresh input per iteration and is excluded from both the count and the timing.
/// The per-iteration secondary metrics are printed to stderr after the Criterion report.
pub fn bench<I, O>(
    c: &mut Criterion<Measurement>,
    group: &str,
    name: &str,
    budget: Budget,
    mut setup: impl FnMut() -> I,
    mut routine: impl FnMut(I) -> O,
) {
    let allocations = measure(&mut setup, &mut routine);
    let report = MetricReport::new();

    println!(
        "alloc {group}/{name}: count={} bytes={}",
        allocations.count, allocations.bytes
    );

    match budget {
        Budget::Report => {}
        Budget::Zero => assert_eq!(allocations.count, 0, "{group}/{name} must not allocate"),
        Budget::AtMost(limit) => assert!(
            allocations.count <= limit,
            "{group}/{name} allocated {} times, budget is {limit}",
            allocations.count
        ),
    }

    c.benchmark_group(group).bench_function(name, |bencher| {
        let mut iterations = 0;

        report.discard_pending();
        bencher.iter_batched(
            &mut setup,
            |input| {
                iterations += 1;
                routine(input)
            },
            BatchSize::SmallInput,
        );
        report.record_sample(iterations);
    });
    report.print("  ");
}

#[cfg(test)]
mod tests;
