use super::*;

#[test]
fn memory_summarises_only_samples_inside_the_span() {
    let samples = [
        Sample { at_ms: 0, rss: 100 },
        Sample {
            at_ms: 10,
            rss: 200,
        },
        Sample {
            at_ms: 20,
            rss: 400,
        },
        Sample {
            at_ms: 30,
            rss: 900,
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
