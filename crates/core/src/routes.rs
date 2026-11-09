use crate::{ReqBody, Request, Response};

/// Generated top-level router: a segment `match` over controller prefixes.
pub trait Routes: Send + Sync + 'static {
    /// Routes the request and always produces a response, including 404.
    fn dispatch<B: ReqBody>(&self, req: Request<B>) -> impl Future<Output = Response> + Send;
}
