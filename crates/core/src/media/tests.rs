use super::*;

const OFFERS: [MediaType; 3] = [
    MediaType::new("application/json"),
    MediaType::new("application/yaml"),
    MediaType::new("text/html"),
];

fn pick(accept: Option<&str>) -> Option<usize> {
    negotiate(accept, OFFERS.len(), |index| OFFERS[index])
}

#[test]
fn negotiate_missing_header_picks_first_offer() {
    assert_eq!(pick(None), Some(0));
}

#[test]
fn negotiate_highest_weight_wins() {
    assert_eq!(
        pick(Some("application/json;q=0.5, application/yaml")),
        Some(1)
    );
}

#[test]
fn negotiate_wildcard_tie_keeps_server_order() {
    assert_eq!(pick(Some("*/*")), Some(0));
}

#[test]
fn negotiate_specific_range_overrides_wildcard_weight() {
    assert_eq!(pick(Some("*/*;q=0.1, text/html")), Some(2));
}

#[test]
fn negotiate_zero_weight_excludes_offer() {
    assert_eq!(
        pick(Some("application/json;q=0, application/yaml;q=0.2")),
        Some(1)
    );
}

#[test]
fn negotiate_no_match_returns_none() {
    assert_eq!(pick(Some("image/png")), None);
}

#[test]
fn match_content_type_ignores_parameters_and_case() {
    let found = match_content_type("Application/JSON; charset=utf-8", OFFERS.len(), |i| {
        OFFERS[i]
    });

    assert_eq!(found, Some(0));
}
