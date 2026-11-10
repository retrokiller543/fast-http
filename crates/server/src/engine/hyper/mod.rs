use std::{convert::Infallible, pin::pin, sync::Arc};

use ::hyper::{body::Incoming, rt::Executor, service::service_fn};
use fast_http_core::{Routes, with_peer};
use fast_http_transport::{ConnInfo, Connection};
use http_body_util::Limited;
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    server::conn::auto,
};

use crate::{Engine, EngineError, Settings, Shutdown, TaskWrap};

/// hyper's minimum HTTP/1 read buffer; smaller values make it panic.
const MIN_H1_BUFFER: usize = 8 * 1024;

/// hyper-based engine serving HTTP/1.1 and HTTP/2 on every connection. Cleartext HTTP/2 is
/// detected through its prior-knowledge preface; over TLS the client's ALPN choice decides.
///
/// HTTP/1 runs with `half_close` enabled so hyper reuses its read buffer across keep-alive
/// requests (otherwise it reallocates ~8 KiB per request). The tradeoff: a handler future is no
/// longer cancelled early when the client fully closes or resets mid-request — the disconnect
/// surfaces only at the next write or next head read. This wastes work on abandoned requests and
/// delays disconnect detection for long-poll, SSE and streaming handlers; it adds no exhaustion
/// vector, since idle and half-open connections stay bounded by the header-read timeout.
#[derive(Debug, Clone, Copy, Default)]
pub struct Hyper;

/// [`Hyper`]'s per-server state: the configured hyper builder and request limits.
pub struct HyperState<W> {
    builder: auto::Builder<Spawner<W>>,
    max_body_bytes: usize,
    peer_addr: bool,
}

/// hyper executor that spawns HTTP/2 stream tasks on tokio, wrapped by `W`.
#[derive(Debug, Clone)]
pub struct Spawner<W>(W);

impl<W, F> Executor<F> for Spawner<W>
where
    W: TaskWrap,
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, task: F) {
        tokio::spawn(self.0.wrap(task));
    }
}

impl Engine for Hyper {
    type State<W: TaskWrap> = HyperState<W>;

    fn new<W: TaskWrap>(settings: &Settings, tasks: W) -> HyperState<W> {
        let mut builder = auto::Builder::new(Spawner(tasks));

        // half_close(true) skips hyper's mid-message EOF probe between keep-alive requests. That
        // probe reads while the just-parsed request head is still shared with the read buffer, so
        // BytesMut::reserve cannot reclaim it and allocates a fresh 8 KiB per request. Tradeoff:
        // the handler is no longer cancelled early on a client full close/RST (see `Hyper` docs).
        builder
            .http1()
            .timer(TokioTimer::new())
            .half_close(true)
            .header_read_timeout(settings.header_read_timeout)
            .max_buf_size(settings.max_header_bytes.max(MIN_H1_BUFFER))
            .max_headers(settings.max_headers);
        builder
            .http2()
            .timer(TokioTimer::new())
            .max_concurrent_streams(settings.max_concurrent_streams)
            .max_header_list_size(u32::try_from(settings.max_header_bytes).unwrap_or(u32::MAX))
            .keep_alive_interval(settings.h2_keep_alive_interval);

        HyperState {
            builder,
            max_body_bytes: settings.max_body_bytes,
            peer_addr: settings.peer_addr,
        }
    }

    async fn serve<W: TaskWrap, C: Connection, R: Routes>(
        state: &HyperState<W>,
        conn: C,
        info: ConnInfo,
        routes: Arc<R>,
        mut shutdown: Shutdown,
    ) -> Result<(), EngineError> {
        let peer = info.peer.filter(|_| state.peer_addr);
        let max_body_bytes = state.max_body_bytes;
        let service = service_fn(move |req: http::Request<Incoming>| {
            let routes = Arc::clone(&routes);

            async move {
                let req = req.map(|body| Limited::new(body, max_body_bytes));

                Ok::<_, Infallible>(with_peer(peer, routes.dispatch(req)).await)
            }
        });
        let mut conn = pin!(
            state
                .builder
                .serve_connection_with_upgrades(TokioIo::new(conn), service)
        );

        tokio::select! {
            result = conn.as_mut() => result.map_err(EngineError),
            () = shutdown.wait() => {
                conn.as_mut().graceful_shutdown();

                conn.await.map_err(EngineError)
            }
        }
    }
}
