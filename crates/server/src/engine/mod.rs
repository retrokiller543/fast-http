use std::sync::Arc;

use fast_http_core::{BoxError, Routes};
use fast_http_transport::{ConnInfo, Connection};
use tokio::sync::watch;

use crate::{Settings, TaskWrap};

mod hyper;

pub use self::hyper::{Hyper, HyperState, Spawner};

/// Protocol engine: speaks HTTP over one established connection.
///
/// The implementing type is a marker; the server builds one [`State`](Engine::State) per run,
/// specialised for the [`TaskWrap`] that wraps the tasks the engine spawns.
pub trait Engine: 'static {
    /// Per-server engine state for task wrapper `W`.
    type State<W: TaskWrap>: Send + Sync + 'static;

    /// Builds the state once per server from its settings and task wrapper.
    fn new<W: TaskWrap>(settings: &Settings, tasks: W) -> Self::State<W>;

    /// Serves requests on `conn` until the peer closes it. When `shutdown` fires, stops taking
    /// new requests, finishes in-flight ones and returns.
    fn serve<W: TaskWrap, C: Connection, R: Routes>(
        state: &Self::State<W>,
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
