use syn::parse_quote;

use super::*;

fn gate(attrs: Vec<Attribute>) -> String {
    Gate::of(&attrs).expect("parses").key().replace(' ', "")
}

#[test]
fn of_collects_plain_cfg_predicates() {
    let key = gate(vec![
        parse_quote!(#[cfg(feature = "a")]),
        parse_quote!(#[doc = "kept"]),
        parse_quote!(#[cfg(unix)]),
    ]);

    assert_eq!(key, "all(feature=\"a\",unix)");
}

#[test]
fn of_turns_cfg_attr_into_the_condition_it_removes_under() {
    let key = gate(vec![
        parse_quote!(#[cfg_attr(not(feature = "foo"), cfg(any()))]),
    ]);

    assert_eq!(key, "all(any(not(not(feature=\"foo\")),all(any())))");
}

#[test]
fn of_resolves_nested_cfg_attr_and_ignores_other_attributes() {
    let nested = gate(vec![parse_quote!(#[cfg_attr(a, cfg_attr(b, cfg(c)))])]);
    let unrelated = gate(vec![parse_quote!(#[cfg_attr(a, allow(dead_code))])]);

    assert_eq!(nested, "all(any(not(a),all(any(not(b),all(c)))))");
    assert_eq!(unrelated, "all()");
}

#[test]
fn any_of_is_open_when_one_gate_is() {
    let gated = Gate::of(&[parse_quote!(#[cfg(a)])]).expect("parses");

    assert!(any_of(&[gated.clone(), Gate::default()]).is_none());
    assert!(any_of(&[gated]).is_some());
}
