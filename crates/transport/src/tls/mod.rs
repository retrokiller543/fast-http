use std::{
    io::{self, IoSlice},
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use rustls::{
    ServerConfig,
    crypto::ring,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

use crate::{Accepted, Alpn, ConnInfo, Connection, Listener, Transport};

/// Failures while building a TLS server configuration.
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    /// A PEM input could not be read or parsed.
    #[error("invalid PEM {what}: {source}")]
    Pem {
        /// Which input failed, a file path or `certificate chain`/`private key`.
        what: String,
        /// Underlying PEM error.
        source: rustls::pki_types::pem::Error,
    },

    /// The certificate input contained no certificate.
    #[error("no certificate found in the certificate chain")]
    NoCertificates,

    /// rustls rejected the certificate, key or protocol settings.
    #[error(transparent)]
    Rustls(#[from] rustls::Error),
}

/// Builds a rustls server configuration with the ring provider, safe default protocol versions
/// and ALPN offering `h2` then `http/1.1`.
pub fn server_config(cert_chain_pem: &[u8], key_pem: &[u8]) -> Result<ServerConfig, TlsError> {
    let certs = CertificateDer::pem_slice_iter(cert_chain_pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| pem_error("certificate chain", source))?;
    let key = PrivateKeyDer::from_pem_slice(key_pem)
        .map_err(|source| pem_error("private key", source))?;

    build(certs, key)
}

/// [`server_config`] reading PEM files.
pub fn server_config_from_files(
    cert_chain: impl AsRef<Path>,
    key: impl AsRef<Path>,
) -> Result<ServerConfig, TlsError> {
    let cert_chain = cert_chain.as_ref();
    let key = key.as_ref();
    let certs = CertificateDer::pem_file_iter(cert_chain)
        .and_then(Iterator::collect::<Result<Vec<_>, _>>)
        .map_err(|source| pem_error(path_name(cert_chain), source))?;
    let key =
        PrivateKeyDer::from_pem_file(key).map_err(|source| pem_error(path_name(key), source))?;

    build(certs, key)
}

fn build(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<ServerConfig, TlsError> {
    if certs.is_empty() {
        return Err(TlsError::NoCertificates);
    }

    let mut config = ServerConfig::builder_with_provider(Arc::new(ring::default_provider()))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    config.alpn_protocols = Alpn::ALL.iter().map(|alpn| alpn.id().to_vec()).collect();

    Ok(config)
}

fn pem_error(what: impl Into<String>, source: rustls::pki_types::pem::Error) -> TlsError {
    TlsError::Pem {
        what: what.into(),
        source,
    }
}

fn path_name(path: &Path) -> String {
    PathBuf::from(path).display().to_string()
}

/// TLS over any inner transport, using rustls.
#[derive(Clone)]
pub struct Tls<T> {
    inner: T,
    acceptor: TlsAcceptor,
}

impl<T> Tls<T> {
    /// Wraps `inner`; every connection is handshaken with `config`.
    pub fn new(inner: T, config: Arc<ServerConfig>) -> Self {
        Self {
            inner,
            acceptor: TlsAcceptor::from(config),
        }
    }
}

impl<T: Transport> Transport for Tls<T> {
    type Addr = T::Addr;
    type Listener = TlsListener<T::Listener>;

    async fn bind(&self, addr: T::Addr) -> io::Result<Self::Listener> {
        let inner = self.inner.bind(addr).await?;

        Ok(TlsListener {
            inner,
            acceptor: self.acceptor.clone(),
        })
    }
}

/// Listener that hands out connections still awaiting their TLS handshake.
pub struct TlsListener<L> {
    inner: L,
    acceptor: TlsAcceptor,
}

impl<L: Listener> Listener for TlsListener<L> {
    type Addr = L::Addr;
    type Accepted = TlsAccepted<L::Accepted>;

    async fn accept(&mut self) -> io::Result<Self::Accepted> {
        let inner = self.inner.accept().await?;

        Ok(TlsAccepted {
            inner,
            acceptor: self.acceptor.clone(),
        })
    }

    fn local_addr(&self) -> io::Result<L::Addr> {
        self.inner.local_addr()
    }
}

/// A connection whose TLS handshake has not run yet.
pub struct TlsAccepted<A> {
    inner: A,
    acceptor: TlsAcceptor,
}

impl<A: Accepted> Accepted for TlsAccepted<A> {
    type Conn = TlsConn<A::Conn>;

    async fn establish(self) -> io::Result<Self::Conn> {
        let conn = self.inner.establish().await?;
        let info = conn.info();
        let stream = self.acceptor.accept(conn).await?;
        let alpn = stream.get_ref().1.alpn_protocol().and_then(Alpn::from_id);

        Ok(TlsConn {
            stream,
            info: ConnInfo {
                alpn,
                secure: true,
                ..info
            },
        })
    }
}

/// An established TLS connection.
pub struct TlsConn<C> {
    stream: TlsStream<C>,
    info: ConnInfo,
}

impl<C: Connection> Connection for TlsConn<C> {
    fn info(&self) -> ConnInfo {
        self.info
    }
}

impl<C: Connection> AsyncRead for TlsConn<C> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl<C: Connection> AsyncWrite for TlsConn<C> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests;
