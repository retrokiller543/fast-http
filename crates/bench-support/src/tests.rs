use super::*;

#[test]
fn measure_counts_one_boxed_u64() {
    let allocations = measure(|| (), |()| Box::new(0u64));

    assert_eq!(allocations, Allocations { count: 1, bytes: 8 });
}

#[test]
fn measure_empty_routine_allocates_nothing() {
    let allocations = measure(|| (), |()| ());

    assert_eq!(allocations, Allocations::default());
}
