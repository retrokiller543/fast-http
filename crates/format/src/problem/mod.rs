use fast_http_core::{ErrorResponse, IntoResponse, Response, ResponseBody};
use http::{HeaderValue, StatusCode, header::CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// RFC 9457 problem details; opt-in, nothing in the framework requires this format.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Problem {
    /// URI identifying the problem type; `about:blank` when omitted.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    /// Short, human-readable summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// HTTP status code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,

    /// Explanation specific to this occurrence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    /// URI identifying this occurrence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,

    /// Extension members.
    #[serde(flatten)]
    pub extensions: Map<String, Value>,
}

impl Problem {
    /// Creates a problem for `status`, titled with its canonical reason.
    pub fn new(status: StatusCode) -> Self {
        Self {
            title: status.canonical_reason().map(str::to_owned),
            status: Some(status.as_u16()),
            ..Self::default()
        }
    }

    /// Sets the occurrence-specific explanation.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());

        self
    }

    /// Sets the problem type URI.
    pub fn kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());

        self
    }

    /// Adds an extension member.
    pub fn extension(mut self, name: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(name.into(), value.into());

        self
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let status = self
            .status
            .and_then(|code| StatusCode::from_u16(code).ok())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = serde_json::to_vec(&self).unwrap_or_default();
        let mut response = Response::new(ResponseBody::from(body));

        *response.status_mut() = status;
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );

        response
    }
}

/// An [`ErrorResponse`] that can describe itself as RFC 9457 problem details.
///
/// Implement [`ErrorResponse`] as usual and point its `IntoResponse` at the problem:
///
/// ```ignore
/// impl IntoResponse for MyError {
///     fn into_response(self) -> Response { self.into_problem_response() }
/// }
/// impl ErrorResponse for MyError { fn status(&self) -> StatusCode { StatusCode::NOT_FOUND } }
/// impl ProblemErrorResponse for MyError {}
/// ```
pub trait ProblemErrorResponse: ErrorResponse {
    /// The problem document; defaults to the status plus the error text as detail.
    fn problem(&self) -> Problem {
        Problem::new(self.status()).detail(self.to_string())
    }

    /// Renders [`ProblemErrorResponse::problem`] as `application/problem+json`.
    fn into_problem_response(self) -> Response
    where
        Self: Sized,
    {
        self.problem().into_response()
    }
}

#[cfg(test)]
mod tests;
