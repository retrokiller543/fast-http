use std::{io, net::SocketAddr};

use tokio::net::TcpStream;

use crate::{Accepted, ConnInfo, Connection, Listener, Transport};

/// Plain TCP over tokio sockets.
#[derive(Debug, Clone, Copy)]
pub struct Tcp {
    nodelay: bool,
}

impl Tcp {
    /// TCP with `TCP_NODELAY` enabled, which suits request/response traffic.
    pub const fn new() -> Self {
        Self { nodelay: true }
    }

    /// Sets `TCP_NODELAY` on every accepted socket.
    pub const fn nodelay(mut self, enabled: bool) -> Self {
        self.nodelay = enabled;
        self
    }
}

impl Default for Tcp {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for Tcp {
    type Addr = SocketAddr;
    type Listener = TcpListener;

    async fn bind(&self, addr: SocketAddr) -> io::Result<TcpListener> {
        let inner = tokio::net::TcpListener::bind(addr).await?;

        Ok(TcpListener {
            inner,
            nodelay: self.nodelay,
        })
    }
}

/// Bound TCP socket.
#[derive(Debug)]
pub struct TcpListener {
    inner: tokio::net::TcpListener,
    nodelay: bool,
}

impl TcpListener {
    /// Wraps an already bound tokio listener.
    pub fn from_tokio(inner: tokio::net::TcpListener, nodelay: bool) -> Self {
        Self { inner, nodelay }
    }
}

impl Listener for TcpListener {
    type Addr = SocketAddr;
    type Accepted = TcpStream;

    /// Ignores a failure to set `TCP_NODELAY`: it is a latency hint and the socket still works.
    async fn accept(&mut self) -> io::Result<TcpStream> {
        let (stream, _) = self.inner.accept().await?;

        if self.nodelay {
            let _ = stream.set_nodelay(true);
        }

        Ok(stream)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }
}

impl Accepted for TcpStream {
    type Conn = Self;

    async fn establish(self) -> io::Result<Self> {
        Ok(self)
    }
}

impl Connection for TcpStream {
    /// A socket the peer already reset has no address; the connection fails on first I/O instead.
    fn info(&self) -> ConnInfo {
        ConnInfo::new(self.peer_addr().ok())
    }
}

#[cfg(test)]
mod tests;
