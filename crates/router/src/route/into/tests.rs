use super::*;

#[test]
fn same_path_compares_whole_strings() {
    assert!(same_path("/a/{id}", "/a/{id}"));
    assert!(!same_path("/a/{id}", "/a/{key}"));
    assert!(!same_path("/a", "/a/b"));
}

#[test]
fn attributed_builds_the_route_at_the_pattern() {
    let route = attributed("/users/{id}");

    assert_eq!(route.pattern().names(), ["id"]);
}

#[test]
#[should_panic(expected = "invalid pattern")]
fn attributed_refuses_an_invalid_pattern() {
    attributed("users");
}
