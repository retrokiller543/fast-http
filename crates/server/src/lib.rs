//! Runs [`Routes`](fast_http_core::Routes) on a [`Transport`](fast_http_transport::Transport):
//! the accept loop, a swappable protocol [`Engine`], connection limits and graceful shutdown.

pub mod config;
pub mod engine;
pub mod server;
pub mod task;

#[cfg(test)]
mod testing;

pub use config::{DefaultConfig, ServerConfig, Settings};
pub use engine::{Engine, EngineError, Hyper, HyperState, Shutdown, Spawner};
pub use fast_http_transport as transport;
pub use server::Server;
pub use task::{Plain, TaskWrap};

/// `tracing` target for accept-loop and connection events.
pub const TRACE_TARGET: &str = "fast_http::server";
