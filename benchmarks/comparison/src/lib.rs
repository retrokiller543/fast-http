//! Shared scenarios for the integrated benchmarks: identical apps on fast-http, axum and actix-web, the same
//! route table for every router, and the request cases both the Criterion benches and the
//! instruction counter replay.

pub mod apps;
pub mod cases;
pub mod routing;
