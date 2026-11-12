use quote::quote;
use syn::{Pat, parse::Parser, parse_quote};

use super::*;

fn pat(tokens: proc_macro2::TokenStream) -> Pat {
    Pat::parse_single.parse2(tokens).expect("pattern")
}

fn param(tokens: proc_macro2::TokenStream) -> Param {
    let pat = pat(tokens);

    Param {
        bindings: bindings(&pat),
        pat,
        ty: parse_quote!(()),
    }
}

fn names(pat: &Pat) -> Vec<String> {
    bindings(pat).iter().map(Ident::to_string).collect()
}

#[test]
fn bindings_plain_identifier() {
    assert_eq!(names(&pat(quote!(body))), ["body"]);
}

#[test]
fn bindings_tuple_struct_pattern() {
    assert_eq!(names(&pat(quote!(Query(q)))), ["q"]);
}

#[test]
fn bindings_struct_pattern_with_renames() {
    assert_eq!(names(&pat(quote!(Body { value: req, .. }))), ["req"]);
}

#[test]
fn bindings_nested_tuple() {
    assert_eq!(names(&pat(quote!((a, (b, _))))), ["a", "b"]);
}

#[test]
fn bindings_mutable_and_reference_patterns() {
    assert_eq!(names(&pat(quote!(mut x))), ["x"]);
    assert_eq!(names(&pat(quote!(&y))), ["y"]);
}

#[test]
fn analyze_finds_the_parameters_an_expression_reads() {
    let params = [
        param(quote!(Query(q))),
        param(quote!(headers)),
        param(quote!(body)),
    ];
    let expr: Expr = parse_quote!(q.stream && headers.contains_key("x"));
    let analyzed = analyze(&expr, &params).expect("analyzes");

    assert_eq!(analyzed.uses, [0, 1]);
}

#[test]
fn analyze_sees_names_inside_nested_groups() {
    let params = [param(quote!(body))];
    let expr: Expr = parse_quote!(matches!(body.value.mode, Some(Mode::Fast)));

    assert_eq!(analyze(&expr, &params).expect("analyzes").uses, [0]);
}

#[test]
fn analyze_expression_without_parameters_uses_none() {
    let params = [param(quote!(body))];
    let expr: Expr = parse_quote!(cfg!(debug_assertions));

    assert!(analyze(&expr, &params).expect("analyzes").uses.is_empty());
}

#[test]
fn analyze_rejects_self() {
    let expr: Expr = parse_quote!(self.enabled);
    let error = analyze(&expr, &[]).map(drop).expect_err("error");

    assert!(error.to_string().contains("cannot use `self`"));
}

#[test]
fn analyze_reserved_names_without_a_matching_parameter_are_hints() {
    let expr: Expr = parse_quote!(headers.contains_key("x") && query.contains("y"));
    let analyzed = analyze(&expr, &[]).expect("analyzes");

    assert!(analyzed.reserved == [Reserved::Headers, Reserved::Query]);
}

#[test]
fn analyze_a_parameter_with_the_reserved_name_is_used_instead() {
    let params = [param(quote!(headers))];
    let expr: Expr = parse_quote!(headers.contains_key("x") && query.contains("y"));
    let analyzed = analyze(&expr, &params).expect("analyzes");

    assert_eq!(analyzed.uses, [0]);
    assert!(analyzed.reserved == [Reserved::Query]);
}

#[test]
fn analyze_a_destructured_binding_named_query_also_wins() {
    let params = [param(quote!(Query(query)))];
    let expr: Expr = parse_quote!(query.page == 1);

    assert!(
        analyze(&expr, &params)
            .expect("analyzes")
            .reserved
            .is_empty()
    );
}

#[test]
fn analyze_unrelated_names_are_not_reserved() {
    let expr: Expr = parse_quote!(mode == 1);

    assert!(analyze(&expr, &[]).expect("analyzes").reserved.is_empty());
}

#[test]
fn analyze_ignores_member_path_and_field_names_that_match_a_parameter() {
    let params = [
        param(quote!(contains_key)),
        param(quote!(id)),
        param(quote!(express)),
    ];
    let expr: Expr = parse_quote!(
        headers.contains_key("x") && matches!(job, Job::Fast { id: 0, .. }) && Kind::express()
    );
    let analyzed = analyze(&expr, &params).expect("analyzes");

    assert!(analyzed.uses.is_empty());
}

#[test]
fn analyze_still_finds_bindings_next_to_ranges_and_paths() {
    let params = [param(quote!(low)), param(quote!(high)), param(quote!(id))];
    let expr: Expr = parse_quote!((low..high).contains(&id));
    let analyzed = analyze(&expr, &params).expect("analyzes");

    assert_eq!(analyzed.uses, [0, 1, 2]);
}

#[test]
fn analyze_ignores_a_name_the_guard_binds_itself() {
    let params = [param(quote!(mode)), param(quote!(payload))];
    let expr: Expr = parse_quote!(matches!(mode, Mode::Ready { payload } if payload > 0));
    let analyzed = analyze(&expr, &params).expect("analyzes");

    assert_eq!(analyzed.uses, [0]);
}
