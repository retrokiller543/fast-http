use super::*;

#[test]
fn report_is_read_in_milliseconds_with_failures_and_deadline_aborts_apart() {
    let json = br#"{
        "summary": { "successRate": 0.99, "requestsPerSec": 51234.5 },
        "latencyPercentiles": { "p10": 0.0001, "p50": 0.0002, "p90": 0.0004, "p99": 0.001, "p99.9": 0.002 },
        "statusCodeDistribution": { "200": 990, "404": 7, "503": 3 },
        "errorDistribution": { "connection reset": 2, "aborted due to deadline": 5 }
    }"#;
    let outcome = parse(json).expect("valid report");

    assert_eq!(outcome.responses, 1_000);
    assert_eq!(outcome.unexpected, 10);
    assert_eq!(outcome.errors, 2);
    assert_eq!(outcome.aborted, 5);
    assert_eq!(outcome.throughput, 51234.5);
    assert_eq!(outcome.latency.p50, Some(0.2));
    assert_eq!(outcome.latency.p999, Some(2.0));
}

#[test]
fn report_without_responses_has_no_latency() {
    let json = br#"{
        "summary": { "requestsPerSec": 0.0 },
        "latencyPercentiles": { "p50": null, "p90": null, "p99": null, "p99.9": null },
        "statusCodeDistribution": {},
        "errorDistribution": { "Connection refused (os error 61)": 2 }
    }"#;
    let outcome = parse(json).expect("valid report");

    assert_eq!(outcome.responses, 0);
    assert_eq!(outcome.errors, 2);
    assert_eq!(outcome.latency.p99, None);
}
