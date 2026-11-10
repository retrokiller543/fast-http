use quote::quote;

use super::*;

fn parse(tokens: proc_macro2::TokenStream) -> syn::Result<ControllerArgs> {
    syn::parse2(tokens)
}

fn krate(args: &ControllerArgs) -> String {
    let krate = &args.krate;

    quote!(#krate).to_string().replace(' ', "")
}

#[test]
fn parse_empty_has_no_prefix_and_defaults_to_the_facade() {
    let args = parse(quote!()).expect("parses");

    assert!(args.prefix.is_none());
    assert_eq!(krate(&args), "::fast_http");
}

#[test]
fn parse_prefix_is_kept() {
    let args = parse(quote!("/users")).expect("parses");

    assert_eq!(args.prefix.expect("prefix").value(), "/users");
}

#[test]
fn parse_crate_override_replaces_the_facade() {
    let args = parse(quote!("/users", crate = my::http)).expect("parses");

    assert_eq!(krate(&args), "my::http");
}

#[test]
fn parse_prefix_given_twice_is_an_error() {
    let error = parse(quote!("/a", "/b")).map(drop).expect_err("error");

    assert!(error.to_string().contains("given twice"));
}

#[test]
fn parse_unknown_argument_is_an_error() {
    let error = parse(quote!(prefix = "/a")).map(drop).expect_err("error");

    assert!(error.to_string().contains("expected a path prefix"));
}
