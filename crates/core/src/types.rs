use http_body::Body;

/// HTTP request over a transport-chosen body type, so core never depends on an engine.
pub type Request<B> = http::Request<B>;

/// HTTP response; the default body is the allocation-free [`ResponseBody`](crate::ResponseBody).
pub type Response<B = crate::ResponseBody> = http::Response<B>;

/// Bound every request body must satisfy; engines supply their own concrete type.
pub trait ReqBody: Body<Data: Send, Error: Into<crate::BoxError>> + Send + 'static {}

impl<B> ReqBody for B where B: Body<Data: Send, Error: Into<crate::BoxError>> + Send + 'static {}
