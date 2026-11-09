//! I/O-free building blocks: bodies, extraction, guards, endpoints, controllers and routing.

pub mod body;
pub mod controller;
pub mod endpoint;
pub mod error;
pub mod extract;
pub mod guard;
pub mod media;
pub mod rejection;
pub mod response;
pub mod routes;
pub mod types;

pub use body::{BoxError, ResponseBody, buffer};
pub use controller::Controller;
pub use endpoint::{DispatchOptions, Endpoint, EndpointSet, Selection, dispatch_set};
pub use error::{
    DefaultErrorHandler, ErrorHandler, ErrorResponse, HandlerError, IntoOutcome, Outcome,
};
pub use extract::{
    Extension, FromParams, FromParts, FromPathParam, FromRequest, Path, PathArgs, PathParams,
    PeerAddr, Segments,
};
pub use guard::{BodyGuard, GuardPhase, HeadGuard};
pub use media::MediaType;
pub use rejection::Rejection;
pub use response::IntoResponse;
pub use routes::Routes;
pub use types::{ReqBody, Request, Response};
