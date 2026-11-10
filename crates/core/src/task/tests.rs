use std::{
    pin::pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Waker},
};

use super::*;

fn counting() -> (TaskSpawner, Arc<AtomicUsize>) {
    let spawned = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&spawned);
    let spawner = TaskSpawner::new(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
    });

    (spawner, spawned)
}

fn poll_once<F: Future>(future: F) -> Poll<F::Output> {
    let mut future = pin!(future);

    future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn current_outside_a_request_is_none() {
    assert!(TaskSpawner::current().is_none());
}

#[test]
fn current_inside_with_spawner_spawns_through_it() {
    let (spawner, spawned) = counting();
    let future = with_spawner(Some(spawner), async {
        TaskSpawner::current()
            .expect("spawner is current")
            .spawn(async {});
    });

    assert!(poll_once(future).is_ready());
    assert_eq!(spawned.load(Ordering::SeqCst), 1);
}

#[test]
fn spawner_is_restored_after_each_poll() {
    let (spawner, _) = counting();

    assert!(poll_once(with_spawner(Some(spawner), async {})).is_ready());
    assert!(TaskSpawner::current().is_none());
}

#[test]
fn inner_none_hides_the_outer_spawner() {
    let (spawner, _) = counting();
    let hidden = Arc::new(Mutex::new(None));
    let sink = Arc::clone(&hidden);
    let outer = with_spawner(Some(spawner), async move {
        let inner = with_spawner(None, async move {
            *sink.lock().expect("lock") = Some(TaskSpawner::current().is_none());
        });

        inner.await;
    });

    assert!(poll_once(outer).is_ready());
    assert_eq!(*hidden.lock().expect("lock"), Some(true));
}

#[test]
fn from_parts_without_a_spawner_is_rejected() {
    let parts = http::Request::new(()).into_parts().0;

    assert!(TaskSpawner::from_parts(&parts).is_err());
}
