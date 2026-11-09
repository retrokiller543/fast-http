//! Flow combinators and tower adapters built on the [`Flow`](fast_http_core::Flow) contract.
//!
//! Flows attach to a whole router with `Router::layer` or to one route with `Route::layer`.

pub mod branch;
pub mod map;
#[cfg(test)]
mod testing;
#[cfg(feature = "tower")]
pub mod tower;

pub use branch::Branch;
pub use fast_http_core::{Flow, Identity, Next, Stack, reject};
pub use map::{MapRequest, MapResponse, map_request, map_response};
#[cfg(feature = "tower")]
pub use tower::{BodyError, NextService, TowerBody, TowerFlow};
