use std::task::Waker;

use super::*;

#[test]
fn poll_frame_bytes_yields_payload_then_ends() {
    let mut body = ResponseBody::from("hello");
    let mut cx = Context::from_waker(Waker::noop());

    let first = Pin::new(&mut body).poll_frame(&mut cx);
    let second = Pin::new(&mut body).poll_frame(&mut cx);

    let Poll::Ready(Some(Ok(frame))) = first else {
        panic!("expected a data frame");
    };

    assert_eq!(
        frame.into_data().expect("frame should be data"),
        Bytes::from_static(b"hello")
    );
    assert!(matches!(second, Poll::Ready(None)));
}

#[test]
fn size_hint_empty_is_exact_zero() {
    let body = ResponseBody::Empty;

    assert!(body.is_end_stream());
    assert_eq!(body.size_hint().exact(), Some(0));
}
