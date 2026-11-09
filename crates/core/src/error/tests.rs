use super::*;
use crate::Rejection;

fn parts() -> Parts {
    http::Request::new(()).into_parts().0
}

#[test]
fn handler_error_rejection_keeps_status_and_downcasts() {
    let error = HandlerError::new(Rejection::BodyTooLarge { limit: 8 });

    assert_eq!(error.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(error.downcast_ref::<Rejection>().is_some());
}

#[test]
fn handler_error_internal_hides_detail() {
    let error = HandlerError::internal("db password leaked");

    let response = error.respond(&parts());

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn into_outcome_result_err_converts_to_handler_error() {
    let result: Result<&'static str, Rejection> = Err(Rejection::NotFound);

    let outcome = result.into_outcome(&parts());

    assert_eq!(
        outcome.expect_err("should be an error").status(),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn into_outcome_plain_response_is_ok() {
    let outcome = StatusCode::CREATED.into_outcome(&parts());

    assert_eq!(outcome.expect("should be ok").status(), StatusCode::CREATED);
}
