//! Global allocator that counts allocations while the harness has counting switched on.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
};

use serde::{Deserialize, Serialize};

static ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

/// The system allocator, counting allocations and requested bytes while counting is on. A
/// `realloc` counts as one allocation of its new size. While off, an allocation costs one relaxed
/// load of a flag that is rarely written, so throughput runs are not skewed by shared counters.
#[derive(Debug, Clone, Copy, Default)]
pub struct Counting;

/// Allocations and bytes counted while counting was on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    /// Number of allocations, reallocations included.
    pub allocs: u64,
    /// Bytes requested by those allocations.
    pub bytes: u64,
}

fn record(size: usize) {
    if ENABLED.load(Relaxed) {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(size as u64, Relaxed);
    }
}

// SAFETY: every method forwards to `System` with the caller's arguments unchanged; counting only
// touches atomics and never allocates.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());

        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller upholds `GlobalAlloc::dealloc`'s contract.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());

        // SAFETY: the caller upholds `GlobalAlloc::alloc_zeroed`'s contract.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(new_size);

        // SAFETY: the caller upholds `GlobalAlloc::realloc`'s contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Resets the counters and starts counting.
pub fn start() {
    ALLOCS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    ENABLED.store(true, Relaxed);
}

/// Stops counting and returns what was counted since [`start`].
pub fn stop() -> Counts {
    ENABLED.store(false, Relaxed);

    Counts {
        allocs: ALLOCS.load(Relaxed),
        bytes: BYTES.load(Relaxed),
    }
}
