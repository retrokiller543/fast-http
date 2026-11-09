use std::{
    pin::pin,
    task::{Context, Poll, Waker},
};

use http::StatusCode;

use super::*;
use crate::{DefaultErrorHandler, Rejection, ResponseBody};

struct Echo;

impl Next for Echo {
    async fn run<B: ReqBody>(self, req: Request<B>) -> Response {
        let mut response = Response::new(ResponseBody::Empty);

        response.headers_mut().extend(req.headers().clone());

        response
    }
}

struct Push(&'static str);

impl Flow for Push {
    async fn handle<B: ReqBody, N: Next, H: ErrorHandler>(
        &self,
        mut req: Request<B>,
        next: N,
        _: &H,
    ) -> Response {
        req.headers_mut()
            .append("x-order", http::HeaderValue::from_static(self.0));

        next.run(req).await
    }
}

fn drive<F: Flow>(flow: &F) -> Response {
    let request = Request::new(http_body_util::Empty::<bytes::Bytes>::new());
    let mut cx = Context::from_waker(Waker::noop());
    let Poll::Ready(response) = pin!(run(flow, request, Echo, &DefaultErrorHandler)).poll(&mut cx)
    else {
        panic!("flows without I/O complete immediately");
    };

    response
}

#[test]
fn stack_runs_outer_before_inner() {
    let flow = Stack::new(Push("inner"), Push("outer"));

    let response = drive(&flow);

    let order: Vec<_> = response.headers().get_all("x-order").iter().collect();

    assert_eq!(order, ["outer", "inner"]);
}

#[test]
fn reject_renders_through_error_handler() {
    let request = Request::new(http_body_util::Empty::<bytes::Bytes>::new());
    let mut cx = Context::from_waker(Waker::noop());

    let Poll::Ready(response) =
        pin!(reject(&DefaultErrorHandler, request, Rejection::NotFound)).poll(&mut cx)
    else {
        panic!("default handler completes immediately");
    };

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
