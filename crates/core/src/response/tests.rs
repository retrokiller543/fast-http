use super::*;

#[test]
fn into_response_status_tuple_overrides_inner_status() {
    let response = (StatusCode::CREATED, "made").into_response();

    assert_eq!(response.status(), StatusCode::CREATED);
}
