use std::{convert::Infallible, pin::pin, sync::Arc};

use ::hyper::{body::Incoming, service::service_fn};
use fast_http_core::{PeerAddr, Routes};
use fast_http_transport::{ConnInfo, Connection};
use http_body_util::Limited;
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::conn::auto,
};

use crate::{Engine, EngineError, Settings, Shutdown};

/// hyper's minimum HTTP/1 read buffer; smaller values make it panic.
const MIN_H1_BUFFER: usize = 8 * 1024;

/// hyper-based engine serving HTTP/1.1 and HTTP/2 on every connection. Cleartext HTTP/2 is
/// detected through its prior-knowledge preface; over TLS the client's ALPN choice decides.
#[derive(Clone)]
pub struct Hyper {
    builder: auto::Builder<TokioExecutor>,
    max_body_bytes: usize,
    peer_addr: bool,
}

impl Engine for Hyper {
    fn new(settings: &Settings) -> Self {
        let mut builder = auto::Builder::new(TokioExecutor::new());

        builder
            .http1()
            .timer(TokioTimer::new())
            .header_read_timeout(settings.header_read_timeout)
            .max_buf_size(settings.max_header_bytes.max(MIN_H1_BUFFER))
            .max_headers(settings.max_headers);
        builder
            .http2()
            .timer(TokioTimer::new())
            .max_concurrent_streams(settings.max_concurrent_streams)
            .max_header_list_size(u32::try_from(settings.max_header_bytes).unwrap_or(u32::MAX))
            .keep_alive_interval(settings.h2_keep_alive_interval);

        Self {
            builder,
            max_body_bytes: settings.max_body_bytes,
            peer_addr: settings.peer_addr,
        }
    }

    async fn serve<C: Connection, R: Routes>(
        &self,
        conn: C,
        info: ConnInfo,
        routes: Arc<R>,
        mut shutdown: Shutdown,
    ) -> Result<(), EngineError> {
        let peer = info.peer.filter(|_| self.peer_addr).map(PeerAddr);
        let max_body_bytes = self.max_body_bytes;
        let service = service_fn(move |mut req: http::Request<Incoming>| {
            let routes = Arc::clone(&routes);

            async move {
                if let Some(peer) = peer {
                    req.extensions_mut().insert(peer);
                }

                let req = req.map(|body| Limited::new(body, max_body_bytes));

                Ok::<_, Infallible>(routes.dispatch(req).await)
            }
        });
        let mut conn = pin!(
            self.builder
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
