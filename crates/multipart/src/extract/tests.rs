use bytes::Bytes;
use fast_http_core::{Response, Routes};
use fast_http_router::{Route, Router};
use http::{Request, StatusCode};
use http_body_util::{BodyExt, Full, Limited};

use super::*;

const BODY: &str = "--X\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\nhello\r\n--X\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.txt\"\r\nContent-Type: text/plain\r\n\r\nfile body\r\n--X--\r\n";

fn parts(content_type: Option<&str>) -> Parts {
    let mut builder = Request::builder();

    if let Some(content_type) = content_type {
        builder = builder.header(CONTENT_TYPE, content_type);
    }

    builder.body(()).expect("valid request").into_parts().0
}

async fn extract<const WHOLE: usize, const FIELD: usize>(
    content_type: Option<&str>,
    body: &'static str,
) -> Result<Multipart<WHOLE, FIELD>, MultipartError> {
    Multipart::from_request(
        &parts(content_type),
        Full::new(Bytes::from_static(body.as_bytes())),
    )
    .await
}

#[tokio::test]
async fn next_field_yields_fields_in_order() {
    let mut multipart = extract::<1024, 1024>(Some("multipart/form-data; boundary=X"), BODY)
        .await
        .expect("multipart");

    let note = multipart.next_field().await.expect("ok").expect("field");

    assert_eq!(note.name(), Some("note"));
    assert_eq!(note.file_name(), None);
    assert_eq!(note.text().await.expect("text"), "hello");

    let file = multipart.next_field().await.expect("ok").expect("field");

    assert_eq!(file.file_name(), Some("a.txt"));
    assert_eq!(file.content_type(), Some("text/plain"));
    assert_eq!(
        file.bytes().await.expect("bytes"),
        Bytes::from_static(b"file body")
    );
    assert!(multipart.next_field().await.expect("ok").is_none());
}

#[tokio::test]
async fn chunk_streams_a_field() {
    let mut multipart = extract::<1024, 1024>(Some("multipart/form-data; boundary=X"), BODY)
        .await
        .expect("multipart");
    let mut field = multipart.next_field().await.expect("ok").expect("field");
    let mut collected = Vec::new();

    while let Some(chunk) = field.chunk().await.expect("chunk") {
        collected.extend_from_slice(&chunk);
    }

    assert_eq!(collected, b"hello");
}

#[tokio::test]
async fn from_request_without_content_type_is_not_multipart() {
    let result = extract::<1024, 1024>(None, BODY).await;

    assert!(matches!(result, Err(MultipartError::NotMultipart)));
}

#[tokio::test]
async fn from_request_json_content_type_is_not_multipart() {
    let result = extract::<1024, 1024>(Some("application/json"), BODY).await;

    assert!(matches!(result, Err(MultipartError::NotMultipart)));
}

#[tokio::test]
async fn from_request_without_boundary_is_rejected() {
    let result = extract::<1024, 1024>(Some("multipart/form-data"), BODY).await;

    assert!(matches!(result, Err(MultipartError::MissingBoundary)));
}

#[tokio::test]
async fn field_over_limit_is_too_large() {
    let mut multipart = extract::<1024, 4>(Some("multipart/form-data; boundary=X"), BODY)
        .await
        .expect("multipart");
    let field = multipart.next_field().await.expect("ok").expect("field");

    assert!(matches!(field.text().await, Err(MultipartError::TooLarge)));
}

#[tokio::test]
async fn whole_body_over_limit_is_too_large() {
    let mut multipart = extract::<16, 1024>(Some("multipart/form-data; boundary=X"), BODY)
        .await
        .expect("multipart");

    assert!(matches!(
        multipart.next_field().await,
        Err(MultipartError::TooLarge)
    ));
}

#[tokio::test]
async fn server_body_limit_surfaces_as_too_large() {
    let body = Limited::new(Full::new(Bytes::from_static(BODY.as_bytes())), 16);
    let mut multipart: Multipart =
        Multipart::from_request(&parts(Some("multipart/form-data; boundary=X")), body)
            .await
            .expect("multipart");

    assert!(matches!(
        multipart.next_field().await,
        Err(MultipartError::TooLarge)
    ));
}

#[tokio::test]
async fn truncated_body_is_malformed() {
    let mut multipart = extract::<1024, 1024>(
        Some("multipart/form-data; boundary=X"),
        "--X\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\nno end",
    )
    .await
    .expect("multipart");
    let field = multipart.next_field().await.expect("ok").expect("field");

    assert!(matches!(
        field.text().await,
        Err(MultipartError::Malformed(_))
    ));
}

async fn upload(mut multipart: Multipart) -> Result<String, MultipartError> {
    let mut names = Vec::new();

    while let Some(field) = multipart.next_field().await? {
        names.push(field.name().unwrap_or("-").to_owned());
    }

    Ok(names.join(","))
}

#[tokio::test]
async fn handler_takes_multipart_as_last_argument() {
    let router = Router::new().route(Route::new("/upload").expect("pattern").post(upload));
    let request = Request::builder()
        .method("POST")
        .uri("/upload")
        .header(CONTENT_TYPE, "multipart/form-data; boundary=X")
        .body(Full::new(Bytes::from_static(BODY.as_bytes())))
        .expect("request");

    let response: Response = router.dispatch(request).await;
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"note,file"));
}
