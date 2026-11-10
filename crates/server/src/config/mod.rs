use std::time::Duration;

/// Static server preset. Every const is a default that [`Settings`] copies and the server can
/// override at runtime.
pub trait ServerConfig: Send + Sync + 'static {
    /// Open connections; the accept loop pauses while the limit is reached.
    const MAX_CONNECTIONS: usize = 10_000;
    /// Request head size. HTTP/1 raises values below 8 KiB to 8 KiB, hyper's minimum buffer.
    const MAX_HEADER_BYTES: usize = 16 * 1024;
    /// HTTP/1 header count.
    const MAX_HEADERS: usize = 100;
    /// Request body bytes read from the wire, enforced while streaming.
    const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
    /// Concurrent HTTP/2 streams per connection.
    const MAX_CONCURRENT_STREAMS: u32 = 128;
    /// Time allowed to establish a connection, including the TLS handshake.
    const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
    /// Time allowed to receive an HTTP/1 request head, which also bounds idle keep-alive.
    const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(30);
    /// Interval of HTTP/2 PING keep-alives; `None` disables them.
    const H2_KEEP_ALIVE_INTERVAL: Option<Duration> = None;
    /// Time in-flight requests get to finish after shutdown starts.
    const DRAIN_TIMEOUT: Duration = Duration::from_secs(30);
    /// Whether handlers can extract the connection's [`PeerAddr`](fast_http_core::PeerAddr). It
    /// is carried around each poll of the request, so it costs no allocation.
    const PEER_ADDR: bool = true;
}

/// The stock preset.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultConfig;

impl ServerConfig for DefaultConfig {}

/// Runtime server settings, seeded from a [`ServerConfig`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// See [`ServerConfig::MAX_CONNECTIONS`].
    pub max_connections: usize,
    /// See [`ServerConfig::MAX_HEADER_BYTES`].
    pub max_header_bytes: usize,
    /// See [`ServerConfig::MAX_HEADERS`].
    pub max_headers: usize,
    /// See [`ServerConfig::MAX_BODY_BYTES`].
    pub max_body_bytes: usize,
    /// See [`ServerConfig::MAX_CONCURRENT_STREAMS`].
    pub max_concurrent_streams: u32,
    /// See [`ServerConfig::HANDSHAKE_TIMEOUT`].
    pub handshake_timeout: Duration,
    /// See [`ServerConfig::HEADER_READ_TIMEOUT`].
    pub header_read_timeout: Duration,
    /// See [`ServerConfig::H2_KEEP_ALIVE_INTERVAL`].
    pub h2_keep_alive_interval: Option<Duration>,
    /// See [`ServerConfig::DRAIN_TIMEOUT`].
    pub drain_timeout: Duration,
    /// See [`ServerConfig::PEER_ADDR`].
    pub peer_addr: bool,
}

impl Settings {
    /// The settings `C` describes.
    pub const fn of<C: ServerConfig>() -> Self {
        Self {
            max_connections: C::MAX_CONNECTIONS,
            max_header_bytes: C::MAX_HEADER_BYTES,
            max_headers: C::MAX_HEADERS,
            max_body_bytes: C::MAX_BODY_BYTES,
            max_concurrent_streams: C::MAX_CONCURRENT_STREAMS,
            handshake_timeout: C::HANDSHAKE_TIMEOUT,
            header_read_timeout: C::HEADER_READ_TIMEOUT,
            h2_keep_alive_interval: C::H2_KEEP_ALIVE_INTERVAL,
            drain_timeout: C::DRAIN_TIMEOUT,
            peer_addr: C::PEER_ADDR,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::of::<DefaultConfig>()
    }
}
