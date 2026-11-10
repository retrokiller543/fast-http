use bytes::Bytes;
use tokio_tungstenite::tungstenite::{
    Message as Raw,
    protocol::{CloseFrame as RawClose, frame::coding::CloseCode},
};

pub use tokio_tungstenite::tungstenite::Utf8Bytes;

/// Registered close codes of RFC 6455, for [`CloseFrame::code`].
pub mod close_code {
    /// The purpose of the connection has been fulfilled.
    pub const NORMAL: u16 = 1000;
    /// The endpoint is going away, such as a server shutting down.
    pub const AWAY: u16 = 1001;
    /// The endpoint terminates the connection because of a protocol error.
    pub const PROTOCOL: u16 = 1002;
    /// The endpoint received data of a type it cannot accept.
    pub const UNSUPPORTED: u16 = 1003;
    /// Reserved: no status code was present.
    pub const STATUS: u16 = 1005;
    /// Reserved: the connection closed without a close frame.
    pub const ABNORMAL: u16 = 1006;
    /// The endpoint received data that was inconsistent with the message type.
    pub const INVALID: u16 = 1007;
    /// The endpoint received a message that violates its policy.
    pub const POLICY: u16 = 1008;
    /// The endpoint received a message that is too big to process.
    pub const SIZE: u16 = 1009;
    /// The client expected the server to negotiate an extension it did not.
    pub const EXTENSION: u16 = 1010;
    /// The server hit an unexpected condition.
    pub const ERROR: u16 = 1011;
    /// The server is restarting.
    pub const RESTART: u16 = 1012;
    /// The server is overloaded; the client may reconnect later.
    pub const AGAIN: u16 = 1013;
}

/// Payload of a close frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseFrame {
    /// Status code, see [`close_code`].
    pub code: u16,
    /// Human readable reason.
    pub reason: Utf8Bytes,
}

/// A WebSocket message as the application sees it; fragmentation is handled below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// A UTF-8 text message.
    Text(Utf8Bytes),
    /// A binary message.
    Binary(Bytes),
    /// A ping; the peer is answered with a pong automatically.
    Ping(Bytes),
    /// A pong, usually an answer to a ping.
    Pong(Bytes),
    /// A close frame; after it no more data arrives.
    Close(Option<CloseFrame>),
}

impl Message {
    /// A text message.
    pub fn text(text: impl Into<Utf8Bytes>) -> Self {
        Self::Text(text.into())
    }

    /// A binary message.
    pub fn binary(data: impl Into<Bytes>) -> Self {
        Self::Binary(data.into())
    }

    /// The payload as bytes, whatever the kind of message.
    pub fn into_data(self) -> Bytes {
        match self {
            Self::Text(text) => Bytes::from(text),
            Self::Binary(data) | Self::Ping(data) | Self::Pong(data) => data,
            Self::Close(Some(frame)) => Bytes::from(frame.reason),
            Self::Close(None) => Bytes::new(),
        }
    }

    pub(crate) fn from_raw(raw: Raw) -> Option<Self> {
        match raw {
            Raw::Text(text) => Some(Self::Text(text)),
            Raw::Binary(data) => Some(Self::Binary(data)),
            Raw::Ping(data) => Some(Self::Ping(data)),
            Raw::Pong(data) => Some(Self::Pong(data)),
            Raw::Close(frame) => Some(Self::Close(frame.map(|frame| CloseFrame {
                code: frame.code.into(),
                reason: frame.reason,
            }))),
            Raw::Frame(_) => None,
        }
    }

    pub(crate) fn into_raw(self) -> Raw {
        match self {
            Self::Text(text) => Raw::Text(text),
            Self::Binary(data) => Raw::Binary(data),
            Self::Ping(data) => Raw::Ping(data),
            Self::Pong(data) => Raw::Pong(data),
            Self::Close(frame) => Raw::Close(frame.map(|frame| RawClose {
                code: CloseCode::from(frame.code),
                reason: frame.reason,
            })),
        }
    }
}

#[cfg(test)]
mod tests;
