use bytes::Bytes;
use fast_http_core::FromRequest;
use http_body_util::{BodyExt, Full};

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

fn source(replay: bool) -> BodySource<Full<Bytes>> {
    BodySource::new(Full::new(Bytes::from_static(b"payload")), replay)
}

#[tokio::test]
async fn take_without_replay_hands_over_the_stream_once() {
    let mut source = source(false);
    let first = source.take().await.expect("first");
    let second = source.take().await.expect("second");

    assert_eq!(first.collect().await.expect("body").to_bytes(), "payload");
    assert!(second.collect().await.expect("body").to_bytes().is_empty());
}

#[tokio::test]
async fn take_with_replay_gives_every_reader_the_bytes() {
    let mut source = source(true);
    let first = source.take().await.expect("first");
    let second = source.take().await.expect("second");

    assert_eq!(first.collect().await.expect("body").to_bytes(), "payload");
    assert_eq!(second.collect().await.expect("body").to_bytes(), "payload");
}

#[tokio::test]
async fn replay_reports_the_exact_size_once_buffered() {
    let mut source = source(true);
    let body = source.take().await.expect("body");

    assert_eq!(body.size_hint().exact(), Some(7));
}

#[tokio::test]
async fn extract_last_reads_the_body_through_the_source() {
    let mut source = source(false);
    let params = PathParams::new(&[], &[]);
    let parts = parts();
    let Text(text) = extract_last::<Text, _, _>(&parts, &params, &mut source)
        .await
        .expect("extracted");

    assert_eq!(text, "payload");
}

#[tokio::test]
async fn extract_last_head_extractor_leaves_the_body_alone() {
    let mut source = source(false);
    let params = PathParams::new(&[], &[]);
    let parts = parts();

    let _ = extract_last::<&Parts, _, _>(&parts, &params, &mut source)
        .await
        .expect("extracted");

    let body = source.take().await.expect("body still there");

    assert_eq!(body.collect().await.expect("body").to_bytes(), "payload");
}

#[tokio::test]
async fn fill_extracts_only_when_the_slot_is_empty() {
    let mut slot = None;
    let mut runs = 0;

    for _ in 0..2 {
        fill(&mut slot, async {
            runs += 1;

            Ok::<_, HandlerError>(7_u8)
        })
        .await
        .expect("filled");
    }

    assert_eq!(slot, Some(7));
    assert_eq!(runs, 1);
}

#[tokio::test]
async fn uses_body_tells_body_extractors_from_head_extractors() {
    assert!(uses_body::<Text, _>());
    assert!(!uses_body::<&Parts, _>());
}
