use super::*;

#[test]
fn from_multer_stream_size_exceeded_is_too_large() {
    let error = multer::Error::StreamSizeExceeded { limit: 1 };

    assert!(matches!(
        MultipartError::from_multer(error),
        MultipartError::TooLarge
    ));
}

#[test]
fn from_multer_incomplete_stream_is_malformed() {
    assert!(matches!(
        MultipartError::from_multer(multer::Error::IncompleteStream),
        MultipartError::Malformed(_)
    ));
}

#[test]
fn status_maps_each_variant() {
    assert_eq!(
        ErrorResponse::status(&MultipartError::NotMultipart),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        ErrorResponse::status(&MultipartError::TooLarge),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        ErrorResponse::status(&MultipartError::MissingBoundary),
        StatusCode::BAD_REQUEST
    );
}
