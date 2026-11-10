//! Core building blocks: negotiation, endpoint selection, dispatch and body buffering.

use bytes::Bytes;
use criterion::{Criterion, criterion_group, criterion_main};
use fast_http_bench_support::{Budget, Measurement, bench, criterion, ready};
use fast_http_core::{
    DefaultErrorHandler, DispatchOptions, Endpoint, ErrorHandler, GuardPhase, HandlerError,
    MediaType, PathParams, Rejection, ReqBody, Response, ResponseBody, buffer, dispatch_set,
    media::{match_content_type, negotiate},
};
use http::{Method, Request, header, request::Parts};
use http_body_util::Full;

const OFFERS: [MediaType; 3] = [
    MediaType::new("application/json"),
    MediaType::new("application/yaml"),
    MediaType::new("text/html"),
];

const ACCEPT: &str = "text/html;q=0.8, application/yaml;q=0.9, application/json;q=0.5, */*;q=0.1";

/// Endpoint that answers with an empty 200 when its method matches and its head guard passes.
struct Fixed {
    method: Method,
    head_ok: bool,
    phase: GuardPhase,
}

impl Endpoint for Fixed {
    fn method(&self) -> &Method {
        &self.method
    }

    fn phase(&self) -> GuardPhase {
        self.phase
    }

    fn head_guard(&self, _: &Parts) -> bool {
        self.head_ok
    }

    async fn call<B: ReqBody, H: ErrorHandler>(
        &self,
        _: &Parts,
        _: PathParams<'_>,
        _: B,
        _: &H,
    ) -> Response {
        Response::new(ResponseBody::Empty)
    }
}

fn fixed(head_ok: bool) -> Fixed {
    Fixed {
        method: Method::GET,
        head_ok,
        phase: GuardPhase::Head,
    }
}

fn request(method: Method, body: &'static [u8]) -> Request<Full<Bytes>> {
    Request::builder()
        .method(method)
        .uri("/x")
        .header(header::ACCEPT, ACCEPT)
        .body(Full::new(Bytes::from_static(body)))
        .expect("valid request")
}

fn media(c: &mut Criterion<Measurement>) {
    bench(
        c,
        "media",
        "negotiate_3_offers",
        Budget::Zero,
        || (),
        |()| negotiate(Some(ACCEPT), OFFERS.len(), |index| OFFERS[index]),
    );
    bench(
        c,
        "media",
        "match_content_type",
        Budget::Zero,
        || (),
        |()| {
            match_content_type("application/yaml; charset=utf-8", OFFERS.len(), |index| {
                OFFERS[index]
            })
        },
    );
}

fn selection(c: &mut Criterion<Measurement>) {
    let options = DispatchOptions::default();
    let one = (fixed(true),);
    let eight = (
        fixed(false),
        fixed(false),
        fixed(false),
        fixed(false),
        fixed(false),
        fixed(false),
        fixed(false),
        fixed(true),
    );

    bench(
        c,
        "dispatch_set",
        "1_endpoint_hit",
        Budget::Zero,
        || request(Method::GET, b""),
        |req| {
            let (parts, body) = req.into_parts();

            ready(dispatch_set(
                &one,
                &parts,
                PathParams::EMPTY,
                body,
                &DefaultErrorHandler,
                options,
            ))
        },
    );
    bench(
        c,
        "dispatch_set",
        "8_endpoints_last_hit",
        Budget::Zero,
        || request(Method::GET, b""),
        |req| {
            let (parts, body) = req.into_parts();

            ready(dispatch_set(
                &eight,
                &parts,
                PathParams::EMPTY,
                body,
                &DefaultErrorHandler,
                options,
            ))
        },
    );
    bench(
        c,
        "dispatch_set",
        "405_with_allow",
        Budget::Report,
        || request(Method::DELETE, b""),
        |req| {
            let (parts, body) = req.into_parts();

            ready(dispatch_set(
                &eight,
                &parts,
                PathParams::EMPTY,
                body,
                &DefaultErrorHandler,
                options,
            ))
        },
    );
}

fn body(c: &mut Criterion<Measurement>) {
    let guarded = (Fixed {
        method: Method::POST,
        head_ok: true,
        phase: GuardPhase::Body,
    },);
    let options = DispatchOptions::default();

    bench(
        c,
        "body",
        "buffer_1kib",
        Budget::Report,
        || Full::new(Bytes::from(vec![0u8; 1024])),
        |body| ready(buffer(body, 1 << 20)),
    );
    bench(
        c,
        "body",
        "dispatch_with_body_phase",
        Budget::Report,
        || request(Method::POST, &[0u8; 64]),
        |req| {
            let (parts, body) = req.into_parts();

            ready(dispatch_set(
                &guarded,
                &parts,
                PathParams::EMPTY,
                body,
                &DefaultErrorHandler,
                options,
            ))
        },
    );
}

fn errors(c: &mut Criterion<Measurement>) {
    bench(
        c,
        "error",
        "box_and_render_rejection",
        Budget::AtMost(2),
        || request(Method::GET, b""),
        |req| {
            let (parts, _) = req.into_parts();
            let error = HandlerError::new(Rejection::NotFound);

            error.respond(&parts)
        },
    );
}

criterion_group! {
    name = benches;
    config = criterion();
    targets = media, selection, body, errors
}
criterion_main!(benches);
