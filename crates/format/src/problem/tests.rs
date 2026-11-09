use std::fmt;

use super::*;

#[derive(Debug)]
struct Missing;

impl fmt::Display for Missing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("no such user")
    }
}

impl std::error::Error for Missing {}

impl IntoResponse for Missing {
    fn into_response(self) -> Response {
        self.into_problem_response()
    }
}

impl ErrorResponse for Missing {
    fn status(&self) -> StatusCode {
        StatusCode::NOT_FOUND
    }
}

impl ProblemErrorResponse for Missing {}

#[test]
fn into_problem_response_uses_status_and_problem_media_type() {
    let response = Missing.into_problem_response();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers()[CONTENT_TYPE], "application/problem+json");
}

#[test]
fn problem_default_carries_title_and_detail() {
    let problem = Missing.problem();

    assert_eq!(problem.title.as_deref(), Some("Not Found"));
    assert_eq!(problem.detail.as_deref(), Some("no such user"));
}
