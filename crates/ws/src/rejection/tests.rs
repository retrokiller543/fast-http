use super::*;

#[test]
fn into_response_method_not_get_sets_allow() {
    let response = WebSocketUpgradeRejection::MethodNotGet.into_response();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(response.headers()[header::ALLOW], "GET");
}

#[test]
fn into_response_unsupported_version_advertises_13() {
    let response = WebSocketUpgradeRejection::UnsupportedVersion.into_response();

    assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
    assert_eq!(response.headers()[header::SEC_WEBSOCKET_VERSION], "13");
}

#[test]
fn status_missing_key_is_bad_request() {
    assert_eq!(
        ErrorResponse::status(&WebSocketUpgradeRejection::MissingKey),
        StatusCode::BAD_REQUEST
    );
}
