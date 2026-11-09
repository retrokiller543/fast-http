use std::{
    pin::Pin,
    task::{Context, Poll, ready},
};

use bytes::Bytes;
use fast_http_core::BoxError;
use futures_core::Stream;
use http_body::{Body, Frame};
use pin_project_lite::pin_project;

mod sse;

#[cfg(feature = "json")]
mod ndjson;

pub use sse::{Encoded, Event, EventData, Sse};

#[cfg(feature = "json")]
pub use ndjson::NdJson;

pin_project! {
    /// Turns a stream of items into body frames with a per-item encoder.
    struct FrameStream<S, F> {
        #[pin]
        stream: S,
        encode: F,
    }
}

impl<S, F, T, E> Body for FrameStream<S, F>
where
    S: Stream<Item = Result<T, E>>,
    F: FnMut(T) -> Result<Bytes, BoxError>,
    E: Into<BoxError>,
{
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();
        let item = ready!(this.stream.poll_next(cx));
        let frame = item.map(|item| match item {
            Ok(item) => (this.encode)(item).map(Frame::data),
            Err(error) => Err(error.into()),
        });

        Poll::Ready(frame)
    }
}
