//! Static, allocation-free request routing over typed controllers.

pub mod dispatch;
pub mod handler;
pub mod pattern;
pub mod route;
pub mod router;

pub use handler::{
    Bind, Bound, Extract, ExtractAt, FnEndpoint, Handler, bind, delete, get, patch, post, put,
};
pub use pattern::{MAX_PARAMS, ParamBuf, Pattern, PatternError, Segment};
pub use route::{Append, Level, Route, RouteList};
pub use router::Router;
