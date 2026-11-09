use bytes::Bytes;
use fast_http_core::{BoxError, IntoResponse, Response, ResponseBody};
use futures_core::Stream;
use http::{HeaderValue, header::CONTENT_TYPE};
use serde::Serialize;

use super::FrameStream;

/// `application/x-ndjson` response: one JSON value per line over a stream of `Result<T, E>`.
pub struct NdJson<S>(pub S);

fn line<T: Serialize>(item: T) -> Result<Bytes, BoxError> {
    let mut encoded = serde_json::to_vec(&item)?;

    encoded.push(b'\n');

    Ok(Bytes::from(encoded))
}

impl<S, T, E> IntoResponse for NdJson<S>
where
    S: Stream<Item = Result<T, E>> + Send + 'static,
    T: Serialize + 'static,
    E: Into<BoxError> + 'static,
{
    fn into_response(self) -> Response {
        let body = FrameStream {
            stream: self.0,
            encode: line::<T>,
        };
        let mut response = Response::new(ResponseBody::stream(body));

        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-ndjson"),
        );

        response
    }
}
