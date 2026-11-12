use syn::parse_quote;

use super::*;

fn names(expr: Expr) -> Vec<String> {
    free_names(&expr).iter().map(Ident::to_string).collect()
}

#[test]
fn free_names_skips_members_paths_and_struct_field_names() {
    let found = names(parse_quote!(
        headers.contains_key("x") && Kind::express() && Point { x: 1, y }.x > 0
    ));

    assert_eq!(found, ["headers", "y"]);
}

#[test]
fn free_names_skips_names_bound_by_the_pattern_of_matches() {
    let found = names(parse_quote!(
        matches!(mode, Mode::Ready { payload } if payload > limit)
    ));

    assert_eq!(found, ["mode", "limit"]);
}

#[test]
fn free_names_skips_closure_arm_and_let_bindings_within_their_scope() {
    let closure = names(parse_quote!(items.iter().any(|item| item > floor)));
    let arms = names(parse_quote!(match mode {
        Some(level) => level > floor,
        None => level,
    }));
    let if_let = names(parse_quote!(if let Some(id) = id {
        id > 0
    } else {
        id == fallback
    }));
    let block = names(parse_quote!({
        let total = a + b;
        total > c
    }));

    assert_eq!(closure, ["items", "floor"]);
    assert_eq!(arms, ["mode", "floor", "level"]);
    assert_eq!(if_let, ["id", "fallback"]);
    assert_eq!(block, ["a", "b", "c"]);
}

#[test]
fn free_names_reads_other_macros_as_expressions_or_tokens() {
    let parsed = names(parse_quote!(assert_ok!(first + second.len(), third)));
    let scanned = names(parse_quote!(custom!(look at this.thing)));

    assert_eq!(parsed, ["first", "second", "third"]);
    assert_eq!(scanned, ["look", "at", "this"]);
}
