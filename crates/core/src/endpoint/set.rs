use bytes::Bytes;
use http::{Method, request::Parts};

use crate::{Endpoint, ErrorHandler, GuardPhase, PathParams, Rejection, ReqBody, Response};

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

/// The endpoints sharing one path, tried in order. Implemented for tuples of endpoints.
pub trait EndpointSet: Send + Sync + 'static {
    /// Chooses an endpoint from the head, and from the body once it is available.
    fn select(&self, parts: &Parts, body: Option<&Bytes>) -> Selection;

    /// Calls the endpoint at `index`, which must come from [`EndpointSet::select`].
    fn call<B: ReqBody, H: ErrorHandler>(
        &self,
        index: usize,
        parts: &Parts,
        params: PathParams<'_>,
        body: B,
        handler: &H,
    ) -> impl Future<Output = Response> + Send;
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

macro_rules! endpoint_set {
    ($($endpoint:ident : $index:tt),+) => {
        impl<$($endpoint: Endpoint),+> EndpointSet for ($($endpoint,)+) {
            fn select(&self, parts: &Parts, body: Option<&Bytes>) -> Selection {
                let methods = [$(self.$index.method()),+];
                let head_alias = !methods.contains(&&Method::HEAD);
                let mut method_seen = false;

                $(
                    if let Some(selection) =
                        consider(&self.$index, $index, parts, body, head_alias, &mut method_seen)
                    {
                        return selection;
                    }
                )+

                rejection(method_seen, &methods, http::StatusCode::BAD_REQUEST)
            }

            async fn call<Bd: ReqBody, Eh: ErrorHandler>(
                &self,
                index: usize,
                parts: &Parts,
                params: PathParams<'_>,
                body: Bd,
                handler: &Eh,
            ) -> Response {
                match index {
                    $($index => self.$index.call(parts, params, body, handler).await,)+
                    _ => unreachable!("index comes from `EndpointSet::select`"),
                }
            }
        }
    };
}

endpoint_set!(A: 0);
endpoint_set!(A: 0, B: 1);
endpoint_set!(A: 0, B: 1, C: 2);
endpoint_set!(A: 0, B: 1, C: 2, D: 3);
endpoint_set!(A: 0, B: 1, C: 2, D: 3, E: 4);
endpoint_set!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5);
endpoint_set!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6);
endpoint_set!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7);
