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

#[test]
fn into_outcome_status_tuple_overrides_inner_status() {
    let parts = http::Request::new(()).into_parts().0;

    let response = (StatusCode::CREATED, "made")
        .into_outcome(&parts)
        .expect("renders");

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[test]
fn into_outcome_status_tuple_keeps_the_inner_error() {
    let parts = http::Request::new(()).into_parts().0;
    let failing: Result<&str, HandlerError> = Err(HandlerError::internal("broken"));

    assert!((StatusCode::CREATED, failing).into_outcome(&parts).is_err());
}
