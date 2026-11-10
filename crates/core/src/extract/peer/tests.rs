use std::{
    net::SocketAddr,
    panic::{AssertUnwindSafe, catch_unwind},
    pin::pin,
    task::{Context, Poll, Waker},
};

use http::Request;

use super::*;

fn poll<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("test future suspended"),
    }
}

fn addr(text: &str) -> SocketAddr {
    text.parse().expect("addr")
}

fn read() -> Result<PeerAddr, Rejection> {
    let (parts, ()) = Request::new(()).into_parts();

    PeerAddr::from_parts(&parts)
}

#[test]
fn peer_is_read_inside_the_scope_only() {
    let peer = addr("10.0.0.1:4000");

    assert_eq!(
        poll(with_peer(Some(peer), async { read() })),
        Ok(PeerAddr(peer))
    );
    assert!(read().is_err());
}

#[test]
fn extension_wins_over_scope() {
    let inserted = addr("10.0.0.2:5000");
    let mut request = Request::new(());

    request.extensions_mut().insert(PeerAddr(inserted));

    let (parts, ()) = request.into_parts();

    assert_eq!(
        poll(with_peer(Some(addr("10.0.0.1:4000")), async {
            PeerAddr::from_parts(&parts)
        })),
        Ok(PeerAddr(inserted))
    );
}

#[test]
fn nested_scope_restores_the_outer_peer() {
    let outer = addr("10.0.0.1:4000");
    let inner = addr("10.0.0.3:6000");
    let seen = poll(with_peer(Some(outer), async {
        let inside = poll(with_peer(Some(inner), async { read() }));

        (inside, read())
    }));

    assert_eq!(seen, (Ok(PeerAddr(inner)), Ok(PeerAddr(outer))));
}

#[test]
fn panic_restores_the_outer_peer() {
    let outer = addr("10.0.0.1:4000");
    let after = poll(with_peer(Some(outer), async {
        let result = catch_unwind(AssertUnwindSafe(|| {
            poll(with_peer(Some(addr("10.0.0.3:6000")), async {
                panic!("handler panic")
            }))
        }));

        (result.is_err(), read())
    }));

    assert_eq!(after, (true, Ok(PeerAddr(outer))));
}
