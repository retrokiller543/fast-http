use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use fast_http_core::{FromRequest, PeerAddr, Rejection, ReqBody, Routes, buffer};
use fast_http_router::{Route, Router};
use fast_http_transport::{Alpn, Listener, Memory, MemoryConnector};
use http::{Method, Request, request::Parts};
use http_body_util::{BodyExt, Full};
use hyper::{
    body::Incoming,
    client::conn::{http1, http2},
};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls::{
    ClientConfig, RootCertStore,
    crypto::ring,
    pki_types::{CertificateDer, ServerName, pem::PemObject},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Notify, oneshot},
    task::JoinHandle,
};
use tokio_rustls::{TlsConnector, client::TlsStream};

use crate::{Hyper, Server, Settings, TaskWrap};

pub const CA: &[u8] = include_bytes!("../../../fixtures/tls/ca.pem");
pub const CERT: &[u8] = include_bytes!("../../../fixtures/tls/cert.pem");
pub const KEY: &[u8] = include_bytes!("../../../fixtures/tls/key.pem");

/// Notified by handlers that block, once their request is in flight.
pub static STARTED: Notify = Notify::const_new();

tokio::task_local! {
    /// Context a [`Tagged`] wrapper scopes around every spawned task.
    pub static TAG: &'static str;
}

/// Task wrapper that scopes [`TAG`] around each task, like a framework re-entering its context.
#[derive(Clone)]
pub struct Tagged(pub &'static str);

impl TaskWrap for Tagged {
    fn wrap<F>(&self, task: F) -> impl Future<Output = F::Output> + Send + 'static
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        TAG.scope(self.0, task)
    }
}

/// Body buffered without an extractor limit, so only the server limit applies.
pub struct Raw(pub Bytes);

impl FromRequest for Raw {
    type Rejection = Rejection;

    async fn from_request<B: ReqBody>(_: &Parts, body: B) -> Result<Self, Rejection> {
        buffer(body, usize::MAX).await.map(Raw)
    }
}

async fn hello() -> &'static str {
    "hello"
}

async fn peer(parts: &Parts) -> String {
    parts
        .extensions
        .get::<PeerAddr>()
        .map_or_else(|| "none".to_owned(), |peer| peer.0.to_string())
}

async fn echo(Raw(body): Raw) -> Bytes {
    body
}

async fn tag() -> &'static str {
    TAG.try_with(|tag| *tag).unwrap_or("none")
}

async fn slow() -> &'static str {
    STARTED.notify_one();
    tokio::time::sleep(Duration::from_millis(100)).await;

    "slow"
}

async fn stuck() -> &'static str {
    STARTED.notify_one();

    std::future::pending().await
}

pub fn routes() -> impl Routes {
    Router::new()
        .route(Route::new("/hello").expect("pattern").get(hello))
        .route(Route::new("/peer").expect("pattern").get(peer))
        .route(Route::new("/echo").expect("pattern").post(echo))
        .route(Route::new("/slow").expect("pattern").get(slow))
        .route(Route::new("/stuck").expect("pattern").get(stuck))
        .route(Route::new("/tag").expect("pattern").get(tag))
}

/// A server running on a spawned task.
pub struct Running {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl Running {
    /// Starts shutdown and waits for `serve` to return.
    pub async fn stop(self) {
        let _ = self.stop.send(());

        tokio::time::timeout(Duration::from_secs(2), self.task)
            .await
            .expect("server stops in time")
            .expect("serve does not panic");
    }
}

/// Serves the test routes; a dropped stop sender also shuts the server down.
pub fn spawn<L: Listener, W: TaskWrap>(server: Server<L, Hyper, W>) -> Running {
    let (stop, stopped) = oneshot::channel::<()>();
    let task = tokio::spawn(server.serve(routes(), async {
        let _ = stopped.await;
    }));

    Running { stop, task }
}

/// Serves the test routes over an in-memory transport.
pub async fn start(edit: impl FnOnce(&mut Settings)) -> (MemoryConnector, Running) {
    let (memory, connector) = Memory::new(64 * 1024);
    let server = Server::bind(&memory, ())
        .await
        .expect("bind")
        .settings(edit);

    (connector, spawn(server))
}

pub async fn h1<I>(io: I) -> http1::SendRequest<Full<Bytes>>
where
    I: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let (sender, conn) = http1::handshake(TokioIo::new(io))
        .await
        .expect("h1 handshake");

    tokio::spawn(conn);

    sender
}

pub async fn h2<I>(io: I) -> http2::SendRequest<Full<Bytes>>
where
    I: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let (sender, conn) = http2::handshake(TokioExecutor::new(), TokioIo::new(io))
        .await
        .expect("h2 handshake");

    tokio::spawn(conn);

    sender
}

pub fn request(method: Method, path: &str, body: &'static [u8]) -> Request<Full<Bytes>> {
    Request::builder()
        .method(method)
        .uri(format!("http://localhost{path}"))
        .body(Full::new(Bytes::from_static(body)))
        .expect("valid request")
}

pub fn get_request(path: &str) -> Request<Full<Bytes>> {
    request(Method::GET, path, b"")
}

pub async fn text(response: http::Response<Incoming>) -> String {
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    String::from_utf8(body.to_vec()).expect("utf-8 body")
}

/// Client-side TLS handshake trusting the fixture CA and offering `alpn`.
pub async fn tls_connect<I>(io: I, alpn: Alpn) -> TlsStream<I>
where
    I: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let mut roots = RootCertStore::empty();

    for cert in CertificateDer::pem_slice_iter(CA) {
        roots.add(cert.expect("CA parses")).expect("CA is valid");
    }

    let mut config = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();

    config.alpn_protocols = vec![alpn.id().to_vec()];

    TlsConnector::from(Arc::new(config))
        .connect(ServerName::try_from("localhost").expect("name"), io)
        .await
        .expect("client handshake")
}
