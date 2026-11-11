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

fn parse(source: &'static str) -> Pattern {
    Pattern::parse(source).expect("pattern should parse")
}

#[test]
fn join_appends_the_path_to_the_base() {
    let joined = Pattern::join("/library/{shelf}", "/items/{id}").expect("joins");

    assert_eq!(joined.to_string(), "/library/{shelf}/items/{id}");
    assert_eq!(joined.names(), ["shelf", "id"]);
}

#[test]
fn join_with_root_halves_keeps_the_other_half() {
    assert_eq!(
        Pattern::join("/library", "/").expect("joins").to_string(),
        "/library"
    );
    assert_eq!(
        Pattern::join("/", "/items").expect("joins").to_string(),
        "/items"
    );
}

#[test]
fn join_refuses_what_one_pattern_would_refuse() {
    assert_eq!(
        Pattern::join("/a/{id}", "/b/{id}"),
        Err(PatternError::DuplicateParam("/b/{id}", "id"))
    );
    assert_eq!(
        Pattern::join("/a/**", "/b"),
        Err(PatternError::WildcardNotLast("/a/**"))
    );
}

#[test]
fn cmp_specificity_puts_literals_before_params_before_wildcards() {
    let mut patterns = [
        parse("/items/**"),
        parse("/items/{id}"),
        parse("/items/*"),
        parse("/items/orders"),
    ];

    patterns.sort_by(Pattern::cmp_specificity);

    let order: Vec<_> = patterns.iter().map(ToString::to_string).collect();

    assert_eq!(
        order,
        ["/items/orders", "/items/{id}", "/items/*", "/items/**"]
    );
}

#[test]
fn same_shape_ignores_parameter_names_only() {
    assert!(parse("/a/{id}").same_shape(&parse("/a/{key}")));
    assert!(parse("/a/**").same_shape(&parse("/a/{**rest}")));
    assert!(!parse("/a/{id}").same_shape(&parse("/a/b")));
    assert!(!parse("/a").same_shape(&parse("/b")));
}

#[test]
fn display_writes_the_pattern_back() {
    for source in ["/", "/a/{id}/*/{**rest}", "/files/**"] {
        assert_eq!(parse(source).to_string(), source);
    }
}
