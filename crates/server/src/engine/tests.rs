use std::time::Duration;

use super::*;

#[tokio::test]
async fn shutdown_fires_for_every_clone() {
    let (trigger, mut first) = Shutdown::channel();
    let mut second = first.clone();

    trigger.send_replace(true);

    tokio::time::timeout(Duration::from_secs(1), async {
        first.wait().await;
        second.wait().await;
    })
    .await
    .expect("both signals fire");
}

#[tokio::test]
async fn dropped_trigger_counts_as_shutdown() {
    let (trigger, mut signal) = Shutdown::channel();

    drop(trigger);

    tokio::time::timeout(Duration::from_secs(1), signal.wait())
        .await
        .expect("signal fires");
}
