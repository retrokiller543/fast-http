use std::{
    pin::Pin,
    task::{Context, Poll},
};

use crate::{Rejection, ReqBody};
use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};
use http_body_util::{BodyExt, LengthLimitError, Limited, combinators::UnsyncBoxBody};

/// Default cap on a request body, 8 MiB: what the server reads from the wire, what body guards
/// buffer and what the body extractors accept, unless one is configured otherwise.
pub const DEFAULT_BODY_LIMIT: usize = 8 * 1024 * 1024;

/// Boxed error used at the streaming boundary.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Response body; only [`ResponseBody::Stream`] allocates.
#[derive(Default)]
pub enum ResponseBody {
    /// No payload.
    #[default]
    Empty,
    /// A fully buffered payload.
    Bytes(Bytes),
    /// The single boxed variant, used for streaming responses.
    Stream(UnsyncBoxBody<Bytes, BoxError>),
}

impl ResponseBody {
    /// Wraps any body into the streaming variant.
    pub fn stream<B>(body: B) -> Self
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: Into<BoxError>,
    {
        Self::Stream(body.map_err(Into::into).boxed_unsync())
    }
}

impl std::fmt::Debug for ResponseBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("Empty"),
            Self::Bytes(bytes) => f.debug_tuple("Bytes").field(&bytes.len()).finish(),
            Self::Stream(_) => f.write_str("Stream"),
        }
    }
}

impl Body for ResponseBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.get_mut() {
            Self::Empty => Poll::Ready(None),
            Self::Bytes(bytes) if bytes.is_empty() => Poll::Ready(None),
            Self::Bytes(bytes) => Poll::Ready(Some(Ok(Frame::data(std::mem::take(bytes))))),
            Self::Stream(stream) => Pin::new(stream).poll_frame(cx),
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Bytes(bytes) => bytes.is_empty(),
            Self::Stream(stream) => stream.is_end_stream(),
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            Self::Empty => SizeHint::with_exact(0),
            Self::Bytes(bytes) => SizeHint::with_exact(bytes.len() as u64),
            Self::Stream(stream) => stream.size_hint(),
        }
    }
}

impl From<Bytes> for ResponseBody {
    fn from(value: Bytes) -> Self {
        Self::Bytes(value)
    }
}

impl From<Vec<u8>> for ResponseBody {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes(value.into())
    }
}

impl From<String> for ResponseBody {
    fn from(value: String) -> Self {
        Self::Bytes(value.into())
    }
}

impl From<&'static str> for ResponseBody {
    fn from(value: &'static str) -> Self {
        Self::Bytes(Bytes::from_static(value.as_bytes()))
    }
}

/// Buffers a request body into one `Bytes`, failing with [`Rejection::BodyTooLarge`] past `limit`.
pub async fn buffer<B>(body: B, limit: usize) -> Result<Bytes, Rejection>
where
    B: ReqBody,
{
    let collected = Limited::new(body, limit).collect().await.map_err(|error| {
        match error.downcast::<LengthLimitError>() {
            Ok(_) => Rejection::BodyTooLarge { limit },
            Err(error) => Rejection::BodyRead {
                message: error.to_string(),
            },
        }
    })?;

    Ok(collected.to_bytes())
}

#[cfg(test)]
mod tests;
