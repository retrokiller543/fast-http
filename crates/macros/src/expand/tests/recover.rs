use super::*;

#[test]
fn recover_keeps_the_impl_without_route_attributes_and_adds_the_error() {
    let item = quote! {
        impl Api {
            #[doc = "kept"]
            #[get("/a")]
            fn a(&self, q: QueryParams<'_>) -> String { String::new() }
        }
    };
    let error = controller(quote!(), item.clone())
        .map(drop)
        .expect_err("a sync handler");
    let out = recover(item, error).to_string().replace(' ', "");

    assert!(out.contains("fna(&self,q:QueryParams<'_>)->String"));
    assert!(out.contains("#[doc=\"kept\"]"));
    assert!(!out.contains("#[get"));
    assert!(out.contains("compile_error"));
}

#[test]
fn recover_unparsable_input_is_passed_through_with_the_error() {
    let item = quote!(
        fn not_an_impl() {}
    );
    let out = recover(
        item,
        syn::Error::new(proc_macro2::Span::call_site(), "boom"),
    )
    .to_string();

    assert!(out.contains("compile_error"));
}

#[test]
fn recover_keeps_a_module_without_route_attributes() {
    let item = quote! {
        mod items {
            #[get("/a")]
            fn a() -> String { String::new() }
        }
    };
    let error = routes(quote!(), item.clone()).map(drop).expect_err("error");
    let out = recover(item, error).to_string().replace(' ', "");

    assert!(out.contains("fna()->String"));
    assert!(!out.contains("#[get"));
    assert!(out.contains("compile_error"));
}

#[test]
fn route_general_form_is_stripped_like_the_shorthand_on_recovery() {
    let item = quote! {
        impl Api {
            #[route(GET, "/a")]
            fn a(&self) -> String { String::new() }
        }
    };
    let error = controller(quote!(), item.clone())
        .map(drop)
        .expect_err("sync");
    let out = recover(item, error).to_string().replace(' ', "");

    assert!(!out.contains("#[route"));
}
