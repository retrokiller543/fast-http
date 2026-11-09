use std::sync::Arc;

use fast_http_core::{BoxError, Routes};
use fast_http_transport::{ConnInfo, Connection};
use tokio::sync::watch;

use crate::Settings;

mod hyper;

pub use self::hyper::Hyper;

/// Protocol engine: speaks HTTP over one established connection.
pub trait Engine: Send + Sync + Sized + 'static {
    /// Builds the engine once per server from its settings.
    fn new(settings: &Settings) -> Self;

    /// Serves requests on `conn` until the peer closes it. When `shutdown` fires, stops taking
    /// new requests, finishes in-flight ones and returns.
    fn serve<C: Connection, R: Routes>(
        &self,
        conn: C,
        info: ConnInfo,
        routes: Arc<R>,
        shutdown: Shutdown,
    ) -> impl Future<Output = Result<(), EngineError>> + Send;
}

/// A connection-level protocol or I/O failure.
#[derive(Debug, thiserror::Error)]
#[error("connection failed: {0}")]
pub struct EngineError(#[source] pub BoxError);

/// Signal that fires once the server starts shutting down.
#[derive(Debug, Clone)]
pub struct Shutdown(watch::Receiver<bool>);

impl Shutdown {
    /// A trigger and the signal it fires.
    pub fn channel() -> (watch::Sender<bool>, Self) {
        let (trigger, signal) = watch::channel(false);

        (trigger, Self(signal))
    }

    /// Resolves once shutdown starts. A dropped trigger means the server is gone, which also
    /// counts as shutdown, so the error is ignored.
    pub async fn wait(&mut self) {
        let _ = self.0.wait_for(|stopping| *stopping).await;
    }
}

#[cfg(test)]
mod tests;
