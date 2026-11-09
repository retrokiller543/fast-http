use super::*;

#[test]
fn memory_summarises_only_samples_inside_the_span() {
    let samples = [
        Sample {
            at_ms: 0,
            rss: 100,
            cpu_ms: 0,
        },
        Sample {
            at_ms: 10,
            rss: 200,
            cpu_ms: 0,
        },
        Sample {
            at_ms: 20,
            rss: 400,
            cpu_ms: 0,
        },
        Sample {
            at_ms: 30,
            rss: 900,
            cpu_ms: 0,
        },
    ];

    assert_eq!(
        Memory::over(&samples, 10, 20),
        Some(Memory {
            mean: 300,
            peak: 400
        })
    );
    assert_eq!(Memory::over(&samples, 40, 50), None);
}

#[test]
fn cpu_per_request_divides_cpu_spent_in_the_window_by_responses() {
    let samples = [
        Sample {
            at_ms: 0,
            rss: 1,
            cpu_ms: 100,
        },
        Sample {
            at_ms: 1_000,
            rss: 1,
            cpu_ms: 400,
        },
        Sample {
            at_ms: 2_000,
            rss: 1,
            cpu_ms: 900,
        },
    ];

    assert_eq!(cpu_per_request(&samples, 0, 2_000, 1_000), Some(800.0));
    assert_eq!(cpu_per_request(&samples, 0, 2_000, 0), None);
    assert_eq!(cpu_per_request(&samples, 500, 600, 10), None);
}
