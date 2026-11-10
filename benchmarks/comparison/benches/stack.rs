//! Whole stack, request in to response out: the same JSON API on fast-http and on axum.

use criterion::{Criterion, criterion_group, criterion_main};
use fast_http_bench::{apps, cases::CASES};
use fast_http_bench_support::{Budget, Measurement, bench, criterion, ready};
use fast_http_core::Routes;
use tower::ServiceExt;

fn stack(c: &mut Criterion<Measurement>) {
    let fast_http = apps::fast_http();
    let axum = apps::axum();

    for case in CASES {
        let group = format!("stack/{}", case.name);
        let fast_status = ready(fast_http.dispatch(case.request())).status();
        let axum_status = ready(axum.clone().oneshot(case.request()))
            .expect("infallible")
            .status();

        assert_eq!(fast_status, axum_status, "{} status differs", case.name);

        bench(
            c,
            &group,
            "fast_http",
            Budget::Report,
            || case.request(),
            |req| ready(fast_http.dispatch(req)),
        );
        bench(
            c,
            &group,
            "axum",
            Budget::Report,
            || (axum.clone(), case.request()),
            |(app, req)| ready(app.oneshot(req)),
        );
    }
}

criterion_group! {
    name = benches;
    config = criterion();
    targets = stack
}
criterion_main!(benches);
