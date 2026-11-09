use std::{
    io,
    sync::{Mutex, PoisonError},
};

use tokio::{
    io::{DuplexStream, duplex},
    sync::mpsc,
};

use crate::{Accepted, ConnInfo, Connection, Listener, Transport};

/// In-memory transport over `tokio::io::duplex` pipes, for tests without sockets.
#[derive(Debug)]
pub struct Memory {
    incoming: Mutex<Option<mpsc::Receiver<DuplexStream>>>,
}

impl Memory {
    /// Creates the transport and the connector clients dial through. `buffer` is the pipe
    /// capacity in bytes per direction.
    pub fn new(buffer: usize) -> (Self, MemoryConnector) {
        let (sender, receiver) = mpsc::channel(64);
        let transport = Self {
            incoming: Mutex::new(Some(receiver)),
        };

        (transport, MemoryConnector { sender, buffer })
    }
}

impl Transport for Memory {
    type Addr = ();
    type Listener = MemoryListener;

    async fn bind(&self, (): ()) -> io::Result<MemoryListener> {
        let incoming = self
            .incoming
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::AddrInUse, "memory transport already bound")
            })?;

        Ok(MemoryListener { incoming })
    }
}

/// Dials connections into a [`Memory`] listener.
#[derive(Debug, Clone)]
pub struct MemoryConnector {
    sender: mpsc::Sender<DuplexStream>,
    buffer: usize,
}

impl MemoryConnector {
    /// Opens a connection and returns the client end.
    pub async fn connect(&self) -> io::Result<DuplexStream> {
        let (client, server) = duplex(self.buffer);

        self.sender.send(server).await.map_err(|_| {
            io::Error::new(io::ErrorKind::ConnectionRefused, "memory listener dropped")
        })?;

        Ok(client)
    }
}

/// Listener end of a [`Memory`] transport.
#[derive(Debug)]
pub struct MemoryListener {
    incoming: mpsc::Receiver<DuplexStream>,
}

impl Listener for MemoryListener {
    type Addr = ();
    type Accepted = DuplexStream;

    /// Once every connector is dropped no connection can arrive, so this waits forever like an
    /// idle socket instead of failing the accept loop.
    async fn accept(&mut self) -> io::Result<DuplexStream> {
        match self.incoming.recv().await {
            Some(stream) => Ok(stream),
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> io::Result<()> {
        Ok(())
    }
}

impl Accepted for DuplexStream {
    type Conn = Self;

    async fn establish(self) -> io::Result<Self> {
        Ok(self)
    }
}

impl Connection for DuplexStream {
    fn info(&self) -> ConnInfo {
        ConnInfo::default()
    }
}

#[cfg(test)]
mod tests;
