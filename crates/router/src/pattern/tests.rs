use super::*;

fn matched(pattern: &'static str, path: &str) -> Option<Vec<String>> {
    let pattern = Pattern::parse(pattern).expect("pattern should parse");
    let mut buf = ParamBuf::new();

    pattern.matches(path, &mut buf).then(|| {
        buf.as_slice()
            .iter()
            .map(|value| (*value).to_owned())
            .collect()
    })
}

fn strings(values: &[&str]) -> Option<Vec<String>> {
    Some(values.iter().map(|value| (*value).to_owned()).collect())
}

#[test]
fn matches_literal_path_exactly() {
    assert_eq!(matched("/users", "/users"), strings(&[]));
    assert_eq!(matched("/users", "/users/"), None);
    assert_eq!(matched("/users", "/user"), None);
}

#[test]
fn matches_root_only_for_slash() {
    assert_eq!(matched("/", "/"), strings(&[]));
    assert_eq!(matched("/", "/a"), None);
}

#[test]
fn matches_params_and_borrows_values() {
    assert_eq!(
        matched("/users/{id}/posts/{post}", "/users/7/posts/abc"),
        strings(&["7", "abc"])
    );
    assert_eq!(matched("/users/{id}", "/users/"), None);
    assert_eq!(matched("/users/{id}", "/users/7/extra"), None);
}

#[test]
fn matches_wildcard_capturing_rest() {
    assert_eq!(
        matched("/files/{**path}", "/files/a/b/c.txt"),
        strings(&["a/b/c.txt"])
    );
    assert_eq!(matched("/files/{**path}", "/files"), None);
    assert_eq!(matched("/files/{**path}", "/files/"), None);
}

#[test]
fn matches_any_segment_without_capturing() {
    assert_eq!(matched("/a/*/c", "/a/zzz/c"), strings(&[]));
    assert_eq!(matched("/a/*/c", "/a//c"), None);
    assert_eq!(matched("/a/**", "/a/b/c"), strings(&[]));
    assert_eq!(matched("/a/**", "/a"), None);
}

#[test]
fn parse_rejects_invalid_patterns() {
    assert_eq!(
        Pattern::parse("users"),
        Err(PatternError::MissingLeadingSlash("users"))
    );
    assert_eq!(
        Pattern::parse("/a/"),
        Err(PatternError::EmptySegment("/a/"))
    );
    assert_eq!(
        Pattern::parse("/{}"),
        Err(PatternError::UnnamedParam("/{}"))
    );
    assert_eq!(
        Pattern::parse("/{**}"),
        Err(PatternError::UnnamedParam("/{**}"))
    );
    assert_eq!(
        Pattern::parse("/{**a}/b"),
        Err(PatternError::WildcardNotLast("/{**a}/b"))
    );
    assert_eq!(
        Pattern::parse("/{a}/{a}"),
        Err(PatternError::DuplicateParam("/{a}/{a}", "a"))
    );
}

#[test]
fn names_follow_declaration_order() {
    let pattern = Pattern::parse("/a/{x}/b/{**y}").expect("pattern should parse");

    assert_eq!(pattern.names(), ["x", "y"]);
    assert_eq!(pattern.first_literal(), Some("a"));
}
