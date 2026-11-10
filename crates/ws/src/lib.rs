//! WebSocket upgrades for fast-http, shaped after `axum::extract::ws` so a protocol written
//! against one can be driven by the other.
//!
//! A handler takes a [`WebSocketUpgrade`] as an argument and returns the response of
//! [`WebSocketUpgrade::on_upgrade`], which runs a callback with the upgraded [`WebSocket`].

pub mod admission;
pub mod guard;
pub mod message;
pub mod rejection;
pub mod socket;
pub mod upgrade;

pub use admission::{Admission, AdmissionRejection, Permit, ShutdownSignal};
pub use guard::WebSocketRequest;
pub use message::{CloseFrame, Message, Utf8Bytes, close_code};
pub use rejection::WebSocketUpgradeRejection;
pub use socket::{Error, WebSocket};
pub use upgrade::WebSocketUpgrade;

/// `tracing` target for upgrade events.
pub const TRACE_TARGET: &str = "fast_http::ws";
