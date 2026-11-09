use std::marker::PhantomData;

use bytes::Bytes;
use fast_http_core::{BoxError, IntoOutcome, Outcome, Response, ResponseBody};
use futures_core::Stream;
use http::{
    HeaderValue,
    header::{CACHE_CONTROL, CONTENT_TYPE},
    request::Parts,
};

use super::FrameStream;
use crate::Encode;

/// Payload of an SSE `data:` field; implemented for plain strings and [`Encoded`] values.
pub trait EventData: Send + 'static {
    /// Renders the payload as text; newlines become one `data:` line each.
    fn encode(&self, parts: &Parts) -> Result<String, BoxError>;
}

impl EventData for String {
    fn encode(&self, _parts: &Parts) -> Result<String, BoxError> {
        Ok(self.clone())
    }
}

impl EventData for &'static str {
    fn encode(&self, _parts: &Parts) -> Result<String, BoxError> {
        Ok((*self).to_owned())
    }
}

/// A value rendered with format `F`, e.g. `Encoded<Msg, Json>`; the format must produce UTF-8 text.
pub struct Encoded<T, F> {
    value: T,
    format: PhantomData<fn() -> F>,
}

impl<T, F> Encoded<T, F> {
    /// Wraps a value to be encoded with `F` when the event is sent.
    pub fn new(value: T) -> Self {
        Self {
            value,
            format: PhantomData,
        }
    }
}

impl<T: Send + 'static, F: Encode<T>> EventData for Encoded<T, F> {
    fn encode(&self, parts: &Parts) -> Result<String, BoxError> {
        let bytes = F::encode(&self.value, parts)?;

        Ok(String::from_utf8(bytes.into())?)
    }
}

/// One server-sent event with data of type `D`; `Event` alone carries a plain string.
#[derive(Debug, Clone)]
pub struct Event<D = String> {
    data: Option<D>,
    event: Option<String>,
    id: Option<String>,
    retry: Option<u32>,
    comment: Option<String>,
}

impl<D> Event<D> {
    /// Creates an event carrying `data`.
    pub fn new(data: D) -> Self {
        Self {
            data: Some(data),
            event: None,
            id: None,
            retry: None,
            comment: None,
        }
    }

    /// Creates an event with no data, e.g. a comment-only keep-alive.
    pub fn empty() -> Self {
        Self {
            data: None,
            event: None,
            id: None,
            retry: None,
            comment: None,
        }
    }

    /// Sets the event name.
    pub fn event(mut self, event: impl Into<String>) -> Self {
        self.event = Some(event.into());

        self
    }

    /// Sets the last-event id.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());

        self
    }

    /// Sets the client reconnection delay in milliseconds.
    pub fn retry(mut self, millis: u32) -> Self {
        self.retry = Some(millis);

        self
    }

    /// Adds a comment line.
    pub fn comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());

        self
    }
}

impl Event<String> {
    /// Creates an event with plain text data.
    pub fn text(data: impl Into<String>) -> Self {
        Self::new(data.into())
    }
}

impl<T, F> Event<Encoded<T, F>> {
    /// Creates an event whose data is `value` rendered with format `F`.
    pub fn encoded(value: T) -> Self {
        Self::new(Encoded::new(value))
    }
}

#[cfg(feature = "json")]
impl<T> Event<Encoded<T, crate::formats::Json>> {
    /// Creates an event whose data is the JSON encoding of `value`.
    pub fn json(value: T) -> Self {
        Self::encoded(value)
    }
}

impl<D: EventData> Event<D> {
    fn encode(&self, parts: &Parts) -> Result<Bytes, BoxError> {
        let mut out = String::new();

        if let Some(comment) = &self.comment {
            for line in comment.lines() {
                out.push_str(": ");
                out.push_str(line);
                out.push('\n');
            }
        }

        if let Some(event) = &self.event {
            out.push_str("event: ");
            out.push_str(&event.replace(['\r', '\n'], " "));
            out.push('\n');
        }

        if let Some(id) = &self.id {
            out.push_str("id: ");
            out.push_str(&id.replace(['\r', '\n'], " "));
            out.push('\n');
        }

        if let Some(retry) = self.retry {
            out.push_str(&format!("retry: {retry}\n"));
        }

        if let Some(data) = &self.data {
            for line in data.encode(parts)?.lines() {
                out.push_str("data: ");
                out.push_str(line);
                out.push('\n');
            }
        }

        out.push('\n');

        Ok(Bytes::from(out))
    }
}

/// `text/event-stream` response over any stream of `Result<Event<D>, E>`.
///
/// A handler returns it directly, building the stream with whatever stream library it likes:
///
/// ```ignore
/// async fn ticks(&self) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
///     Sse::new(tick_stream.map(|n| Ok(Event::text(n.to_string()))))
/// }
/// ```
pub struct Sse<S>(S);

impl<S> Sse<S> {
    /// Wraps an event stream.
    pub fn new(stream: S) -> Self {
        Self(stream)
    }
}

impl<S, D, E> IntoOutcome for Sse<S>
where
    S: Stream<Item = Result<Event<D>, E>> + Send + 'static,
    D: EventData,
    E: Into<BoxError> + 'static,
{
    fn into_outcome(self, parts: &Parts) -> Outcome {
        let head = parts.clone();
        let body = FrameStream {
            stream: self.0,
            encode: move |event: Event<D>| event.encode(&head),
        };
        let mut response = Response::new(ResponseBody::stream(body));

        response
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));

        Ok(response)
    }
}

#[cfg(test)]
mod tests;
