use std::{
    io,
    pin::Pin,
    task::{Context, Poll, ready},
    time::Duration,
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
        error::ProtocolError,
        protocol::{Role, WebSocketConfig},
    },
};

use crate::{CloseFrame, Message, Permit, ShutdownSignal, TRACE_TARGET};

/// How long [`WebSocket::close`] waits for the peer to answer the close frame.
pub const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

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
    failed: Option<tungstenite::Error>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> WebSocket<S> {
    /// Wraps an already upgraded `io` as the server side of a connection.
    pub async fn from_io(io: S, protocol: Option<HeaderValue>, config: WebSocketConfig) -> Self {
        Self {
            inner: WebSocketStream::from_raw_socket(io, Role::Server, Some(config)).await,
            protocol,
            permit: None,
            failed: None,
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

    /// Closes the connection: sends a close frame without a status, then waits up to
    /// [`CLOSE_TIMEOUT`] for the peer's close reply so the handshake completes before the socket
    /// is dropped. Fails when the connection breaks before the handshake completes; running out of
    /// time is not an error, the socket is dropped either way.
    pub async fn close(self) -> Result<(), Error> {
        self.close_handshake(None).await
    }

    /// Like [`close`](Self::close), with a status code and reason, such as
    /// [`close_code::AWAY`](crate::close_code::AWAY) on shutdown.
    pub async fn close_with(self, frame: CloseFrame) -> Result<(), Error> {
        self.close_handshake(Some(frame)).await
    }

    /// Sends the close frame and reads, discarding anything that is not the peer's close reply,
    /// until the connection ends, all within one [`CLOSE_TIMEOUT`]. The connection ending
    /// normally is success; a read error on the way, such as a reset, is returned.
    ///
    /// The deadline covers sending too, which flushes and can stall on a peer that stopped
    /// reading; when it passes, the socket is dropped, which frees its admission permit.
    ///
    /// When a close is already under way there is nothing to send: a frame queued earlier, such
    /// as by a cancelled [`SinkExt::close`] or the reply to the peer's close, is flushed instead.
    /// A connection that already ended is success when it ended with a close handshake.
    ///
    /// When an earlier read failed, the stream has ended and no reply can be read: the close
    /// frame is still sent when the connection allows it, and the read error is returned.
    async fn close_handshake(mut self, frame: Option<CloseFrame>) -> Result<(), Error> {
        let handshake = async {
            let sent = self.inner.send(Message::Close(frame).into_raw()).await;

            if let Some(error) = self.failed.take() {
                return Err(Error(error));
            }

            match sent {
                Ok(()) | Err(tungstenite::Error::ConnectionClosed) => {}
                Err(tungstenite::Error::AlreadyClosed) => return Ok(()),
                Err(tungstenite::Error::Protocol(ProtocolError::SendAfterClosing)) => {
                    self.inner.flush().await.map_err(Error)?;
                }
                Err(error) => return Err(Error(error)),
            }

            while let Some(read) = self.inner.next().await {
                read.map_err(Error)?;
            }

            Ok(())
        };

        match tokio::time::timeout(CLOSE_TIMEOUT, handshake).await {
            Ok(result) => result,
            Err(_) => {
                tracing::debug!(target: TRACE_TARGET, "websocket close handshake did not finish in time");

                Ok(())
            }
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> Stream for WebSocket<S> {
    type Item = Result<Message, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match ready!(self.inner.poll_next_unpin(cx)) {
                None => return Poll::Ready(None),
                Some(Err(error)) => {
                    if self.failed.is_none() {
                        self.failed = Some(duplicate(&error));
                    }

                    return Poll::Ready(Some(Err(Error(error))));
                }
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

/// A copy of a read error, which `tungstenite::Error` cannot be cloned for, so
/// [`WebSocket::close`] can report how the stream ended after the original went to the reader.
///
/// The variants reads produce are copied exactly, an I/O error with its kind and message. The
/// others, such as TLS or URL errors, only arise while connecting and become an I/O error with
/// their message.
fn duplicate(error: &tungstenite::Error) -> tungstenite::Error {
    match error {
        tungstenite::Error::ConnectionClosed => tungstenite::Error::ConnectionClosed,
        tungstenite::Error::AlreadyClosed => tungstenite::Error::AlreadyClosed,
        tungstenite::Error::AttackAttempt => tungstenite::Error::AttackAttempt,
        tungstenite::Error::Protocol(error) => tungstenite::Error::Protocol(error.clone()),
        tungstenite::Error::Capacity(error) => tungstenite::Error::Capacity(*error),
        tungstenite::Error::Utf8(text) => tungstenite::Error::Utf8(text.clone()),
        tungstenite::Error::Io(error) => {
            tungstenite::Error::Io(io::Error::new(error.kind(), error.to_string()))
        }
        other => tungstenite::Error::Io(io::Error::other(other.to_string())),
    }
}

#[cfg(test)]
mod tests;
