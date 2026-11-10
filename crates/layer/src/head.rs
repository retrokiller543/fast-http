use bytes::Bytes;
use fast_http_core::Request;
use http::request::Parts;
use http_body_util::Empty;

/// Copies the method, URI, version and headers into a request with no body and no extensions.
pub(crate) fn head_only(parts: &Parts) -> Request<Empty<Bytes>> {
    let (mut head, ()) = Request::new(()).into_parts();

    head.method = parts.method.clone();
    head.uri = parts.uri.clone();
    head.version = parts.version;
    head.headers = parts.headers.clone();

    Request::from_parts(head, Empty::new())
}
