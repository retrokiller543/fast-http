//! Shared benchmark helpers: allocation counting next to wall-clock timing, and ready-poll futures.
//!
//! Linking this crate installs `allocation-counter`'s global allocator. It only counts inside
//! [`measure`], so timing runs pay a thread-local check per allocation and nothing else.

use std::{
    future::Future,
    hint::black_box,
    pin::pin,
    task::{Context, Poll, Waker},
};

use criterion::{BatchSize, Criterion};

pub use allocation_counter::AllocationInfo;

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

/// Drives a future that must finish without suspending, which holds for every in-memory request.
pub fn ready<F: Future>(future: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());

    match pin!(future).poll(&mut cx) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("benchmarked future suspended; use only in-memory bodies"),
    }
}

/// Allocations made by one call of `routine`, after two warm-up calls; `setup` runs outside the count.
pub fn measure<I, O>(
    mut setup: impl FnMut() -> I,
    mut routine: impl FnMut(I) -> O,
) -> AllocationInfo {
    for _ in 0..2 {
        drop(black_box(routine(setup())));
    }

    let input = setup();

    allocation_counter::measure(|| {
        drop(black_box(routine(input)));
    })
}

/// Reports allocations, enforces `budget`, then times the routine with Criterion.
///
/// `setup` builds a fresh input per iteration and is excluded from both the count and the timing.
pub fn bench<I, O>(
    c: &mut Criterion,
    group: &str,
    name: &str,
    budget: Budget,
    mut setup: impl FnMut() -> I,
    mut routine: impl FnMut(I) -> O,
) {
    let info = measure(&mut setup, &mut routine);

    println!(
        "alloc {group}/{name}: count={} bytes={} peak_bytes={}",
        info.count_total, info.bytes_total, info.bytes_max
    );

    match budget {
        Budget::Report => {}
        Budget::Zero => assert_eq!(info.count_total, 0, "{group}/{name} must not allocate"),
        Budget::AtMost(limit) => assert!(
            info.count_total <= limit,
            "{group}/{name} allocated {} times, budget is {limit}",
            info.count_total
        ),
    }

    c.benchmark_group(group).bench_function(name, |bencher| {
        bencher.iter_batched(&mut setup, &mut routine, BatchSize::SmallInput);
    });
}
