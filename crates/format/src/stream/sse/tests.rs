use super::*;

#[test]
fn encode_multiline_text_data_emits_one_data_line_each() {
    let parts = http::Request::new(()).into_parts().0;

    let bytes = Event::text("a\nb")
        .event("tick")
        .id("7")
        .encode(&parts)
        .expect("text event should encode");

    assert_eq!(&bytes[..], b"event: tick\nid: 7\ndata: a\ndata: b\n\n");
}

#[test]
fn encode_empty_event_with_comment_is_keep_alive() {
    let parts = http::Request::new(()).into_parts().0;

    let bytes = Event::<String>::empty()
        .comment("ping")
        .encode(&parts)
        .expect("comment event should encode");

    assert_eq!(&bytes[..], b": ping\n\n");
}

#[cfg(feature = "json")]
#[test]
fn encode_json_event_renders_value_as_data() {
    let parts = http::Request::new(()).into_parts().0;

    let bytes = Event::json(vec![1, 2])
        .encode(&parts)
        .expect("json event should encode");

    assert_eq!(&bytes[..], b"data: [1,2]\n\n");
}
