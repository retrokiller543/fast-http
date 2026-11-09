use http::header;

use super::*;

#[test]
fn into_response_method_not_allowed_sets_allow_header() {
    let rejection = Rejection::MethodNotAllowed {
        allow: vec![Method::GET, Method::POST],
    };

    let response = rejection.into_response();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(response.headers()[header::ALLOW], "GET, POST");
}

#[test]
fn status_guard_miss_uses_configured_status() {
    let rejection = Rejection::GuardMiss {
        status: StatusCode::UNPROCESSABLE_ENTITY,
    };

    assert_eq!(rejection.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
