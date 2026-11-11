//! I/O-free building blocks: bodies, extraction, guards, endpoints and routing.

pub use http;

pub mod body;
pub mod endpoint;
pub mod error;
pub mod extract;
pub mod flow;
pub mod guard;
pub mod media;
pub mod prefix;
pub mod rejection;
pub mod response;
pub mod routes;
pub mod task;
pub mod tuple;
pub mod types;

pub use body::{BoxError, DEFAULT_BODY_LIMIT, ResponseBody, buffer};
pub use endpoint::{Attempt, DispatchOptions, Endpoint, EndpointSet, Selection, dispatch_set};
pub use error::{
    DefaultErrorHandler, ErrorHandler, ErrorResponse, HandlerError, IntoOutcome, Outcome,
};
pub use extract::{
    Extension, FromParams, FromParamsAt, FromParts, FromPathParam, FromRequest, Path, PathArgs,
    PathParams, PeerAddr, QueryParams, Segments, Transparent, WithPeer, with_peer,
};
pub use flow::{Flow, Identity, Next, Stack, reject};
pub use guard::{BodyGuard, GuardPhase, HeadGuard};
pub use media::MediaType;
pub use rejection::Rejection;
pub use response::IntoResponse;
pub use routes::Routes;
pub use task::{Plain, Task, TaskSpawner, TaskWrap, WithSpawner, with_spawner};
pub use tuple::TuplePush;
pub use types::{ReqBody, Request, Response};
