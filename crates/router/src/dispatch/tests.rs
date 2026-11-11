use bytes::Bytes;
use fast_http_core::{FromRequest, buffer};
use http_body_util::Full;

use super::*;

struct Text(String);

impl FromRequest for Text {
    type Rejection = Rejection;

    async fn from_request<B: ReqBody>(_: &Parts, body: B) -> Result<Self, Rejection> {
        let bytes = buffer(body, usize::MAX).await?;

        Ok(Self(String::from_utf8(bytes.to_vec()).expect("utf-8")))
    }
}

fn parts() -> Parts {
    http::Request::new(()).into_parts().0
}

fn source() -> BodySource<Full<Bytes>> {
    BodySource::new(Full::new(Bytes::from_static(b"payload")))
}

#[test]
fn take_hands_over_the_body_once() {
    let mut source = source();

    assert!(source.take().is_ok());
    assert!(source.take().is_err());
}

#[tokio::test]
async fn extract_last_reads_the_body_through_the_source() {
    let mut source = source();
    let params = PathParams::new(&[], &[]);
    let parts = parts();
    let Text(text) = extract_last::<Text, _, _>(&parts, &params, &mut source)
        .await
        .expect("extracted");

    assert_eq!(text, "payload");
}

#[tokio::test]
async fn extract_last_head_extractor_leaves_the_body_alone() {
    let mut source = source();
    let params = PathParams::new(&[], &[]);
    let parts = parts();

    let _ = extract_last::<&Parts, _, _>(&parts, &params, &mut source)
        .await
        .expect("extracted");

    assert!(source.take().is_ok());
}

#[tokio::test]
async fn fill_extracts_only_when_the_slot_is_empty() {
    let mut slot = None;
    let mut runs = 0;

    for _ in 0..2 {
        let value = fill(&mut slot, async {
            runs += 1;

            Ok::<_, HandlerError>(7_u8)
        })
        .await
        .expect("filled");

        assert_eq!(*value, 7);
    }

    assert_eq!(runs, 1);
}

#[tokio::test]
async fn fill_failure_leaves_the_slot_empty_and_reports_the_error() {
    let mut slot: Option<u8> = None;
    let result = fill(&mut slot, async {
        Err::<u8, _>(HandlerError::internal("nope"))
    })
    .await;

    assert!(result.is_err());
    assert!(slot.is_none());
}

#[tokio::test]
async fn take_moves_a_filled_value_out_without_extracting_again() {
    let mut slot = Some(3_u8);
    let value = take(&mut slot, async { Ok::<_, HandlerError>(9_u8) })
        .await
        .expect("taken");

    assert_eq!(value, 3);
    assert!(slot.is_none());
}

#[tokio::test]
async fn take_extracts_when_the_slot_is_empty() {
    let mut slot = None;
    let value = take(&mut slot, async { Ok::<_, HandlerError>(9_u8) })
        .await
        .expect("taken");

    assert_eq!(value, 9);
}

#[test]
fn uses_body_tells_body_extractors_from_head_extractors() {
    assert!(uses_body::<Text, _>());
    assert!(!uses_body::<&Parts, _>());
}
