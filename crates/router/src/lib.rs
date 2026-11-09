//! Static, allocation-free request routing over typed controllers.

pub mod handler;
pub mod pattern;
pub mod route;
pub mod router;

pub use handler::{Extract, FnEndpoint, Handler, delete, get, patch, post, put};
pub use pattern::{MAX_PARAMS, ParamBuf, Pattern, PatternError, Segment};
pub use route::{Route, RouteList};
pub use router::Router;
