use super::*;

#[test]
fn raw_round_trip_keeps_text() {
    let message = Message::text("hi");

    assert_eq!(Message::from_raw(message.clone().into_raw()), Some(message));
}

#[test]
fn raw_round_trip_keeps_close_frame() {
    let message = Message::Close(Some(CloseFrame {
        code: close_code::ERROR,
        reason: Utf8Bytes::from_static("boom"),
    }));

    assert_eq!(Message::from_raw(message.clone().into_raw()), Some(message));
}

#[test]
fn into_data_of_text_is_its_bytes() {
    assert_eq!(Message::text("abc").into_data(), Bytes::from_static(b"abc"));
}

#[test]
fn from_raw_drops_raw_frames() {
    let frame = tokio_tungstenite::tungstenite::protocol::frame::Frame::ping(Bytes::new());

    assert_eq!(Message::from_raw(Raw::Frame(frame)), None);
}
