use super::*;

#[test]
fn strip_prefix_segment_boundary_matches() {
    assert_eq!(strip_prefix("/chat", "/chat"), Some(""));
    assert_eq!(strip_prefix("/chat", "/chat/history"), Some("/history"));
}

#[test]
fn strip_prefix_partial_segment_rejected() {
    assert_eq!(strip_prefix("/chat", "/chatter"), None);
}

#[test]
fn strip_prefix_empty_prefix_matches_everything() {
    assert_eq!(strip_prefix("", "/a/b"), Some("/a/b"));
}

#[test]
fn strip_prefix_trailing_slash_covers_descendants() {
    assert_eq!(strip_prefix("/", "/users"), Some("/users"));
    assert_eq!(strip_prefix("/admin/", "/admin/users"), Some("/users"));
    assert_eq!(strip_prefix("/admin/", "/admin"), Some(""));
    assert_eq!(strip_prefix("/admin/", "/administrator"), None);
}
