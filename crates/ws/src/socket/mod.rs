use std::{
    pin::Pin,
    task::{Context, Poll, ready},
};

use futures_util::{Sink, SinkExt, Stream, StreamExt};
use http::HeaderValue;
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        self,
        protocol::{Role, WebSocketConfig},
    },
};

use crate::{Message, Permit, ShutdownSignal};

/// A failure while reading from or writing to a [`WebSocket`].
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct Error(#[from] tungstenite::Error);

impl Error {
    /// The underlying `tungstenite` error.
    pub fn into_inner(self) -> tungstenite::Error {
        self.0
    }
}

/// An upgraded WebSocket connection, as a [`Stream`] of incoming and a [`Sink`] of outgoing
/// [`Message`]s.
///
/// Pings are answered automatically while the stream is polled. [`StreamExt::split`] yields
/// independent halves.
#[derive(Debug)]
pub struct WebSocket<S = TokioIo<Upgraded>> {
    inner: WebSocketStream<S>,
    protocol: Option<HeaderValue>,
    permit: Option<Permit>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> WebSocket<S> {
    /// Wraps an already upgraded `io` as the server side of a connection.
    pub async fn from_io(io: S, protocol: Option<HeaderValue>, config: WebSocketConfig) -> Self {
        Self {
            inner: WebSocketStream::from_raw_socket(io, Role::Server, Some(config)).await,
            protocol,
            permit: None,
        }
    }

    /// Ties an admission [`Permit`] to this connection, freed when the socket is dropped.
    pub fn with_permit(mut self, permit: Permit) -> Self {
        self.permit = Some(permit);

        self
    }

    /// A signal that fires when the [`Admission`](crate::Admission) that admitted this connection
    /// starts shutting down; it never fires for a connection admitted any other way.
    pub fn shutdown_signal(&self) -> ShutdownSignal {
        self.permit
            .as_ref()
            .map(Permit::shutdown_signal)
            .unwrap_or_default()
    }

    /// Receives the next message, or `None` once the connection is closed.
    pub async fn recv(&mut self) -> Option<Result<Message, Error>> {
        self.next().await
    }

    /// Sends a message and flushes it.
    pub async fn send(&mut self, message: Message) -> Result<(), Error> {
        SinkExt::send(self, message).await
    }

    /// The subprotocol negotiated during the upgrade, if any.
    pub fn protocol(&self) -> Option<&HeaderValue> {
        self.protocol.as_ref()
    }

    /// Closes the connection, sending a close frame first.
    pub async fn close(mut self) -> Result<(), Error> {
        SinkExt::close(&mut self).await
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> Stream for WebSocket<S> {
    type Item = Result<Message, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match ready!(self.inner.poll_next_unpin(cx)) {
                None => return Poll::Ready(None),
                Some(Err(error)) => return Poll::Ready(Some(Err(Error(error)))),
                Some(Ok(raw)) => {
                    if let Some(message) = Message::from_raw(raw) {
                        return Poll::Ready(Some(Ok(message)));
                    }
                }
            }
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> Sink<Message> for WebSocket<S> {
    type Error = Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.inner).poll_ready(cx).map_err(Error)
    }

    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Error> {
        Pin::new(&mut self.inner)
            .start_send(item.into_raw())
            .map_err(Error)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.inner).poll_flush(cx).map_err(Error)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.inner).poll_close(cx).map_err(Error)
    }
}

#[cfg(test)]
mod tests;
