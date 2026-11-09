use std::{io, marker::PhantomData, pin::pin, sync::Arc, time::Duration};

use fast_http_core::Routes;
use fast_http_transport::{Accepted, Connection, Listener, Transport};
use tokio::{
    task::{JoinError, JoinSet},
    time::{sleep, timeout},
};
use tracing::{debug, error, warn};

use crate::{Engine, Hyper, Plain, ServerConfig, Settings, Shutdown, TRACE_TARGET, TaskWrap};

/// Pause after an accept error that is not about a single connection, such as running out of
/// file descriptors, so the loop does not spin.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// Accept loop that runs every connection on its own task through engine `E`, with every
/// spawned task wrapped by `W`.
pub struct Server<L, E = Hyper, W = Plain> {
    listener: L,
    settings: Settings,
    tasks: W,
    engine: PhantomData<fn() -> E>,
}

impl<L: Listener> Server<L> {
    /// Binds `transport` to `addr`.
    pub async fn bind<T>(transport: &T, addr: T::Addr) -> io::Result<Self>
    where
        T: Transport<Listener = L>,
    {
        Ok(Self::new(transport.bind(addr).await?))
    }

    /// Serves an already bound listener.
    pub fn new(listener: L) -> Self {
        Self {
            listener,
            settings: Settings::default(),
            tasks: Plain,
            engine: PhantomData,
        }
    }
}

impl<L: Listener, E: Engine, W: TaskWrap> Server<L, E, W> {
    /// Replaces the settings with the preset `C`.
    pub fn config<C: ServerConfig>(mut self) -> Self {
        self.settings = Settings::of::<C>();
        self
    }

    /// Adjusts the settings at runtime.
    pub fn settings(mut self, edit: impl FnOnce(&mut Settings)) -> Self {
        edit(&mut self.settings);
        self
    }

    /// Swaps the protocol engine.
    pub fn engine<F: Engine>(self) -> Server<L, F, W> {
        Server {
            listener: self.listener,
            settings: self.settings,
            tasks: self.tasks,
            engine: PhantomData,
        }
    }

    /// Wraps every task the server spawns, connection and HTTP/2 stream tasks alike, with
    /// `tasks`, so the embedding framework's context reaches handlers.
    pub fn task_wrap<V: TaskWrap>(self, tasks: V) -> Server<L, E, V> {
        Server {
            listener: self.listener,
            settings: self.settings,
            tasks,
            engine: PhantomData,
        }
    }

    /// Address the listener is bound to.
    pub fn local_addr(&self) -> io::Result<L::Addr> {
        self.listener.local_addr()
    }

    /// Serves `routes` until `shutdown` resolves, then stops accepting, lets in-flight requests
    /// finish within the drain timeout and aborts the rest.
    pub async fn serve<R: Routes>(self, routes: R, shutdown: impl Future<Output = ()> + Send) {
        let Self {
            mut listener,
            settings,
            tasks,
            ..
        } = self;
        let engine = Arc::new(E::new(&settings, tasks.clone()));
        let routes = Arc::new(routes);
        let (trigger, signal) = Shutdown::channel();
        let mut connections = JoinSet::new();
        let mut shutdown = pin!(shutdown);

        loop {
            let full = connections.len() >= settings.max_connections;
            let event = tokio::select! {
                biased;

                () = &mut shutdown => Event::Shutdown,
                Some(joined) = connections.join_next(), if !connections.is_empty() => Event::Closed(joined),
                accepted = listener.accept(), if !full => Event::Accepted(accepted),
            };

            match event {
                Event::Shutdown => break,
                Event::Closed(joined) => report(joined),
                Event::Accepted(Ok(accepted)) => {
                    connections.spawn(tasks.wrap(connection::<_, E, _, _>(
                        accepted,
                        Arc::clone(&engine),
                        Arc::clone(&routes),
                        signal.clone(),
                        settings.handshake_timeout,
                    )));
                }
                Event::Accepted(Err(error)) if is_connection_error(&error) => {
                    debug!(target: TRACE_TARGET, %error, "connection failed before accept");
                }
                Event::Accepted(Err(error)) => {
                    error!(target: TRACE_TARGET, %error, "accept failed");

                    tokio::select! {
                        () = sleep(ACCEPT_BACKOFF) => {}
                        () = &mut shutdown => break,
                    }
                }
            }
        }

        drop(listener);
        trigger.send_replace(true);
        drain(&mut connections, settings.drain_timeout).await;
    }
}

/// What woke the accept loop.
enum Event<A> {
    Shutdown,
    Closed(Result<(), JoinError>),
    Accepted(io::Result<A>),
}

async fn connection<A: Accepted, E: Engine, W: TaskWrap, R: Routes>(
    accepted: A,
    engine: Arc<E::State<W>>,
    routes: Arc<R>,
    shutdown: Shutdown,
    handshake_timeout: Duration,
) {
    let conn = match timeout(handshake_timeout, accepted.establish()).await {
        Ok(Ok(conn)) => conn,
        Ok(Err(error)) => {
            debug!(target: TRACE_TARGET, %error, "handshake failed");

            return;
        }
        Err(_) => {
            debug!(target: TRACE_TARGET, "handshake timed out");

            return;
        }
    };
    let info = conn.info();

    if let Err(error) = E::serve(&*engine, conn, info, routes, shutdown).await {
        debug!(target: TRACE_TARGET, peer = ?info.peer, %error, "connection closed with an error");
    }
}

async fn drain(connections: &mut JoinSet<()>, deadline: Duration) {
    let drained = timeout(deadline, async {
        while let Some(joined) = connections.join_next().await {
            report(joined);
        }
    })
    .await;

    if drained.is_err() {
        warn!(
            target: TRACE_TARGET,
            remaining = connections.len(),
            "drain timeout passed, aborting connections"
        );
        connections.shutdown().await;
    }
}

fn report(joined: Result<(), JoinError>) {
    if let Err(error) = joined
        && error.is_panic()
    {
        error!(target: TRACE_TARGET, %error, "connection task panicked");
    }
}

fn is_connection_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
    )
}

#[cfg(test)]
mod tests;
