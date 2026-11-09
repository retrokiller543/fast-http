//! Request cases replayed against every framework.

use bytes::Bytes;
use http::{Method, Request, header};
use http_body_util::Full;

/// One request shape, built fresh for every iteration.
#[derive(Debug, Clone)]
pub struct Case {
    /// Stable identifier used in bench names and on the instruction-counter command line.
    pub name: &'static str,
    /// Request method.
    pub method: Method,
    /// Request target.
    pub uri: &'static str,
    /// JSON request body; empty means no body.
    pub body: &'static str,
}

/// Every stack case: success paths with real JSON work, then the error paths.
pub const CASES: [Case; 7] = [
    Case {
        name: "static_text",
        method: Method::GET,
        uri: "/health",
        body: "",
    },
    Case {
        name: "get_json",
        method: Method::GET,
        uri: "/users/42",
        body: "",
    },
    Case {
        name: "post_json",
        method: Method::POST,
        uri: "/users",
        body: r#"{"name":"Ada Lovelace","email":"ada@example.com","tags":["admin","math"]}"#,
    },
    Case {
        name: "two_params",
        method: Method::GET,
        uri: "/orgs/rust-lang/repos/cargo",
        body: "",
    },
    Case {
        name: "handler_error",
        method: Method::GET,
        uri: "/users/0",
        body: "",
    },
    Case {
        name: "bad_param",
        method: Method::GET,
        uri: "/users/abc",
        body: "",
    },
    Case {
        name: "not_found",
        method: Method::GET,
        uri: "/missing/route",
        body: "",
    },
];

impl Case {
    /// Builds the request with JSON `accept` and `content-type` headers.
    pub fn request(&self) -> Request<Full<Bytes>> {
        Request::builder()
            .method(self.method.clone())
            .uri(self.uri)
            .header(header::ACCEPT, "application/json")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from_static(self.body.as_bytes())))
            .expect("valid request")
    }
}
