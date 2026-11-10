//! Route lookup only: fast-http against matchit and actix-router on the same twelve-route table.

use criterion::{Criterion, criterion_group, criterion_main};
use fast_http_bench::routing::{self, LOOKUPS};
use fast_http_bench_support::{Budget, Measurement, bench, criterion};

fn lookups(c: &mut Criterion<Measurement>) {
    let fast_http = routing::fast_http();
    let matchit = routing::matchit();
    let actix = routing::actix();

    for lookup in LOOKUPS {
        let group = format!("routing/{}", lookup.name);
        let expected = (lookup.route != usize::MAX).then_some(lookup.route);

        assert_eq!(
            routing::fast_http_lookup(&fast_http, lookup.path).map(|hit| hit.0),
            expected,
            "fast-http {}",
            lookup.path
        );
        assert_eq!(
            routing::matchit_lookup(&matchit, lookup.path).map(|hit| hit.0),
            expected,
            "matchit {}",
            lookup.path
        );
        assert_eq!(
            routing::actix_lookup(&actix, lookup.path).map(|hit| hit.0),
            expected,
            "actix-router {}",
            lookup.path
        );

        bench(
            c,
            &group,
            "fast_http",
            Budget::Zero,
            || (),
            |()| routing::fast_http_lookup(&fast_http, lookup.path),
        );
        bench(
            c,
            &group,
            "matchit",
            Budget::Report,
            || (),
            |()| routing::matchit_lookup(&matchit, lookup.path),
        );
        bench(
            c,
            &group,
            "actix_router",
            Budget::Report,
            || (),
            |()| routing::actix_lookup(&actix, lookup.path),
        );
    }
}

criterion_group! {
    name = benches;
    config = criterion();
    targets = lookups
}
criterion_main!(benches);
