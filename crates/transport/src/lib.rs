//! Byte transports: turn an address into a stream of connections that carry bytes. Nothing here
//! knows about HTTP; the server crate runs protocol engines on top.

use std::{io, net::SocketAddr};

use tokio::io::{AsyncRead, AsyncWrite};

pub mod memory;
pub mod tcp;
#[cfg(feature = "tls")]
pub mod tls;

pub use memory::{Memory, MemoryConnector, MemoryListener};
pub use tcp::{Tcp, TcpListener};
#[cfg(feature = "tls")]
pub use tls::{
    Tls, TlsAccepted, TlsConn, TlsError, TlsListener, server_config, server_config_from_files,
};

/// Binds addresses to listeners.
pub trait Transport: Send + Sync + 'static {
    /// Address the transport binds to.
    type Addr: Send;
    /// Listener produced by [`bind`](Transport::bind).
    type Listener: Listener;

    /// Starts listening on `addr`.
    fn bind(&self, addr: Self::Addr) -> impl Future<Output = io::Result<Self::Listener>> + Send;
}

/// Source of incoming connections.
pub trait Listener: Send + 'static {
    /// Address type reported by [`local_addr`](Listener::local_addr).
    type Addr;
    /// A connection that still has to be established, for example by a TLS handshake.
    type Accepted: Accepted;

    /// Waits for the next connection. Must be cancel-safe: servers poll it inside `select!`.
    fn accept(&mut self) -> impl Future<Output = io::Result<Self::Accepted>> + Send;

    /// Address the listener is bound to.
    fn local_addr(&self) -> io::Result<Self::Addr>;
}

/// An accepted connection before it is usable. Servers establish it on the connection's own task,
/// so a slow handshake never stalls the accept loop.
pub trait Accepted: Send + 'static {
    /// The established connection.
    type Conn: Connection;

    /// Completes any handshake and yields the byte stream.
    fn establish(self) -> impl Future<Output = io::Result<Self::Conn>> + Send;
}

/// An established, readiness-based byte stream.
pub trait Connection: AsyncRead + AsyncWrite + Send + Unpin + 'static {
    /// Facts about the connection, read once when it is established.
    fn info(&self) -> ConnInfo;
}

/// Facts about an established connection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConnInfo {
    /// Remote address, when the transport has one.
    pub peer: Option<SocketAddr>,
    /// Protocol chosen through ALPN, when TLS negotiated one.
    pub alpn: Option<Alpn>,
    /// Whether the bytes are encrypted.
    pub secure: bool,
}

impl ConnInfo {
    /// Info for a plaintext connection from `peer`.
    pub const fn new(peer: Option<SocketAddr>) -> Self {
        Self {
            peer,
            alpn: None,
            secure: false,
        }
    }
}

/// HTTP protocols negotiable through ALPN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Alpn {
    /// `http/1.1`
    Http11,
    /// `h2`
    H2,
}

impl Alpn {
    /// Every protocol, in server preference order.
    pub const ALL: [Self; 2] = [Self::H2, Self::Http11];

    /// The ALPN protocol id.
    pub const fn id(self) -> &'static [u8] {
        match self {
            Self::Http11 => b"http/1.1",
            Self::H2 => b"h2",
        }
    }

    /// Parses an ALPN protocol id.
    pub fn from_id(id: &[u8]) -> Option<Self> {
        Self::ALL.into_iter().find(|alpn| alpn.id() == id)
    }
}

#[cfg(test)]
mod tests;
