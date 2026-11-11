//! fast-http behind one dependency: static routing, guards, flows, a hyper-based server and
//! opt-in WebSocket and multipart support, selected with Cargo features.
//!
//! Each workspace crate is re-exported as a module. Features:
//!
//! - format features (`json` by default, `form`, `yaml`, `toml`, `cbor`, `msgpack`, `xml`,
//!   `problem`) choose the data formats of [`format`];
//! - `tower` adds [`layer::TowerFlow`];
//! - `tls` adds rustls support to the [`server`] transports;
//! - `ws` adds [`ws`] and `multipart` adds [`multipart`];
//! - `macros` adds the [`controller`] attribute, whose `when(..)` guards are plain Rust over the
//!   method's own parameters;
//! - `full` turns all of them on.
//!
//! The `routes` example builds routes by hand and the `controller` example uses [`controller`];
//! both show header and body guards.

pub use fast_http_core as core;
pub use fast_http_format as format;
pub use fast_http_layer as layer;
#[cfg(feature = "macros")]
pub use fast_http_macros::controller;
#[cfg(feature = "multipart")]
pub use fast_http_multipart as multipart;
pub use fast_http_router as router;
pub use fast_http_server as server;
pub use fast_http_transport as transport;
#[cfg(feature = "ws")]
pub use fast_http_ws as ws;

#[cfg(test)]
mod tests;
