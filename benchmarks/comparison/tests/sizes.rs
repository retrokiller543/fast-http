//! Upper bounds on the dispatch future of the comparison API; large futures cost memmoves on
//! every poll.

use std::mem::size_of_val;

use fast_http_bench::{apps, cases::CASES};
use fast_http_core::Routes;

#[test]
fn dispatch_future_stays_small() {
    let app = apps::fast_http();

    for case in CASES {
        let size = size_of_val(&app.dispatch(case.request()));

        println!("{}: {size} bytes", case.name);
        assert!(
            size <= 3_456,
            "{} dispatch future is {size} bytes",
            case.name
        );
    }
}
