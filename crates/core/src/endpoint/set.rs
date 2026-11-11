use bytes::Bytes;
use http::{Method, request::Parts};

use crate::{Attempt, Endpoint, ErrorHandler, GuardPhase, PathParams, Rejection, ReqBody};

use super::either::{
    EndpointFuture1, EndpointFuture2, EndpointFuture3, EndpointFuture4, EndpointFuture5,
    EndpointFuture6, EndpointFuture7, EndpointFuture8, EndpointFuture9, EndpointFuture10,
    EndpointFuture11, EndpointFuture12, EndpointFuture13, EndpointFuture14, EndpointFuture15,
    EndpointFuture16,
};

/// Outcome of choosing among the endpoints of one path.
#[derive(Debug, PartialEq, Eq)]
pub enum Selection {
    /// The endpoint at this index handles the request.
    Selected(usize),
    /// A body-phase endpoint is next in line, so the body must be buffered and selection repeated.
    NeedBody,
    /// No endpoint applies.
    Rejected(Rejection),
}

/// The endpoints sharing one path, tried in order. Implemented for `()` and tuples of up to 16 endpoints.
pub trait EndpointSet: Send + Sync + 'static {
    /// Chooses an endpoint from `from` on, using the head, and the body once it is available.
    fn select(&self, from: usize, parts: &Parts, body: Option<&Bytes>) -> Selection;

    /// Offers the request to the endpoint at `index`, which must come from [`EndpointSet::select`].
    fn attempt<'r, B: ReqBody, H: ErrorHandler>(
        &'r self,
        index: usize,
        parts: &'r Parts,
        params: PathParams<'r>,
        body: B,
        handler: &'r H,
    ) -> impl Future<Output = Attempt<B>> + Send + use<'r, B, H, Self>;
}

fn method_applies(method: &Method, requested: &Method, head_alias: bool) -> bool {
    method == requested || (head_alias && *method == Method::GET && *requested == Method::HEAD)
}

fn consider<E: Endpoint>(
    endpoint: &E,
    index: usize,
    parts: &Parts,
    body: Option<&Bytes>,
    head_alias: bool,
    method_seen: &mut bool,
) -> Option<Selection> {
    if !method_applies(endpoint.method(), &parts.method, head_alias) {
        return None;
    }

    *method_seen = true;

    if !endpoint.head_guard(parts) {
        return None;
    }

    match (endpoint.phase(), body) {
        (GuardPhase::Head, _) => Some(Selection::Selected(index)),
        (GuardPhase::Body, None) => Some(Selection::NeedBody),
        (GuardPhase::Body, Some(body)) => endpoint
            .body_guard(parts, body)
            .then_some(Selection::Selected(index)),
    }
}

fn collect_allow(methods: &[&Method]) -> Vec<Method> {
    let mut allow: Vec<Method> = Vec::new();

    for method in methods {
        if !allow.contains(method) {
            allow.push((*method).clone());
        }

        if **method == Method::GET
            && !methods.contains(&&Method::HEAD)
            && !allow.contains(&Method::HEAD)
        {
            allow.push(Method::HEAD);
        }
    }

    allow
}

fn rejection(method_seen: bool, methods: &[&Method], miss: http::StatusCode) -> Selection {
    if method_seen {
        return Selection::Rejected(Rejection::GuardMiss { status: miss });
    }

    Selection::Rejected(Rejection::MethodNotAllowed {
        allow: collect_allow(methods),
    })
}

impl EndpointSet for () {
    fn select(&self, _: usize, _: &Parts, _: Option<&Bytes>) -> Selection {
        Selection::Rejected(Rejection::MethodNotAllowed { allow: Vec::new() })
    }

    async fn attempt<'r, B: ReqBody, H: ErrorHandler>(
        &'r self,
        _: usize,
        _: &'r Parts,
        _: PathParams<'r>,
        _: B,
        _: &'r H,
    ) -> Attempt<B> {
        unreachable!("an empty set never selects an endpoint")
    }
}

macro_rules! endpoint_set {
    ($future:ident; $($endpoint:ident : $index:tt),+) => {
        impl<$($endpoint: Endpoint),+> EndpointSet for ($($endpoint,)+) {
            fn select(&self, from: usize, parts: &Parts, body: Option<&Bytes>) -> Selection {
                let methods = [$(self.$index.method()),+];
                let head_alias = !methods.contains(&&Method::HEAD);
                let mut method_seen = from > 0;

                $(
                    if $index >= from
                        && let Some(selection) =
                            consider(&self.$index, $index, parts, body, head_alias, &mut method_seen)
                    {
                        return selection;
                    }
                )+

                rejection(method_seen, &methods, http::StatusCode::BAD_REQUEST)
            }

            fn attempt<'r, Bd: ReqBody, Eh: ErrorHandler>(
                &'r self,
                index: usize,
                parts: &'r Parts,
                params: PathParams<'r>,
                body: Bd,
                handler: &'r Eh,
            ) -> impl Future<Output = Attempt<Bd>> + Send + use<'r, Bd, Eh, $($endpoint),+> {
                match index {
                    $($index => $future::$endpoint { fut: self.$index.attempt(parts, params, body, handler) },)+
                    _ => unreachable!("index comes from `EndpointSet::select`"),
                }
            }
        }
    };
}

endpoint_set!(EndpointFuture1; A: 0);
endpoint_set!(EndpointFuture2; A: 0, B: 1);
endpoint_set!(EndpointFuture3; A: 0, B: 1, C: 2);
endpoint_set!(EndpointFuture4; A: 0, B: 1, C: 2, D: 3);
endpoint_set!(EndpointFuture5; A: 0, B: 1, C: 2, D: 3, E: 4);
endpoint_set!(EndpointFuture6; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5);
endpoint_set!(EndpointFuture7; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6);
endpoint_set!(EndpointFuture8; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7);
endpoint_set!(EndpointFuture9; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8);
endpoint_set!(EndpointFuture10; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9);
endpoint_set!(EndpointFuture11; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10);
endpoint_set!(EndpointFuture12; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10, L: 11);
endpoint_set!(EndpointFuture13; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10, L: 11, M: 12);
endpoint_set!(EndpointFuture14; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10, L: 11, M: 12, N: 13);
endpoint_set!(EndpointFuture15; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10, L: 11, M: 12, N: 13, O: 14);
endpoint_set!(EndpointFuture16; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10, L: 11, M: 12, N: 13, O: 14, P: 15);
