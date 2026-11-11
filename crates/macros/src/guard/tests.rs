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
