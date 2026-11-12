use std::time::Duration;

use super::*;

#[test]
fn try_admit_up_to_the_limit_then_rejects() {
    let admission = Admission::new(2);
    let first = admission.try_admit().expect("first");
    let second = admission.try_admit().expect("second");

    assert_eq!(
        admission.try_admit().expect_err("third"),
        AdmissionRejection::AtCapacity { limit: 2 }
    );
    assert_eq!(admission.active(), 2);

    drop((first, second));
}

#[test]
fn dropping_a_permit_frees_its_slot() {
    let admission = Admission::new(1);
    let permit = admission.try_admit().expect("first");

    drop(permit);

    assert_eq!(admission.active(), 0);
    assert!(admission.try_admit().is_ok());
}

#[test]
fn zero_limit_admits_nothing() {
    assert!(Admission::new(0).try_admit().is_err());
}

#[test]
fn unlimited_admits_and_counts() {
    let admission = Admission::unlimited();
    let permits: Vec<_> = (0..100)
        .map(|_| admission.try_admit().expect("admitted"))
        .collect();

    assert_eq!(admission.limit(), None);
    assert_eq!(admission.active(), 100);

    drop(permits);
}

#[test]
fn clones_share_one_count() {
    let admission = Admission::new(1);
    let clone = admission.clone();
    let permit = admission.try_admit().expect("first");

    assert!(clone.try_admit().is_err());
    assert_eq!(clone.active(), 1);

    drop(permit);
}

#[test]
fn shutdown_rejects_new_connections() {
    let admission = Admission::new(5);

    admission.shutdown();

    assert_eq!(
        admission.try_admit().expect_err("closed"),
        AdmissionRejection::ShuttingDown
    );
}

#[tokio::test]
async fn shutdown_signal_fires_for_a_live_permit() {
    let admission = Admission::new(5);
    let permit = admission.try_admit().expect("admitted");
    let mut signal = permit.shutdown_signal();

    admission.shutdown();

    tokio::time::timeout(Duration::from_secs(1), signal.wait())
        .await
        .expect("signal fires");
}

#[tokio::test]
async fn shutdown_signal_without_admission_never_fires() {
    let mut signal = ShutdownSignal::default();

    assert!(
        tokio::time::timeout(Duration::from_millis(20), signal.wait())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn drained_waits_for_the_last_permit() {
    let admission = Admission::new(5);
    let permit = admission.try_admit().expect("admitted");
    let waiter = tokio::spawn({
        let admission = admission.clone();

        async move { admission.drained().await }
    });

    tokio::task::yield_now().await;
    assert!(!waiter.is_finished());

    drop(permit);

    tokio::time::timeout(Duration::from_secs(1), waiter)
        .await
        .expect("drains")
        .expect("join");
}

#[test]
fn into_response_at_capacity_is_503_with_retry_after() {
    let response = AdmissionRejection::AtCapacity { limit: 1 }.into_response();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.headers()[RETRY_AFTER], "1");
}
