use quote::quote;

use super::*;

fn expand(args: TokenStream, item: TokenStream) -> syn::Result<String> {
    let args: ControllerArgs = syn::parse2(args)?;
    let item: ItemStruct = syn::parse2(item)?;

    controller_struct(&args, &item).map(|out| out.to_string().replace(' ', ""))
}

fn error(args: TokenStream, item: TokenStream) -> String {
    expand(args, item).map(drop).expect_err("error").to_string()
}

#[test]
fn controller_struct_implements_controller_with_its_path() {
    let out = expand(
        quote!("/library", groups(catalog)),
        quote!(
            struct Library;
        ),
    )
    .expect("expands");

    assert!(out.contains(
        "impl::fast_http::router::ControllerforLibrary{constPATH:&'staticstr=\"/library\";}"
    ));
}

#[test]
fn controller_struct_merges_every_group_and_moves_the_arc_into_the_last() {
    let out = expand(
        quote!("/library", groups(catalog, admin)),
        quote!(
            struct Library;
        ),
    )
    .expect("expands");

    assert!(
        out.contains(".merge(Self::__fast_http_group_catalog(::std::sync::Arc::clone(&self))?)")
    );
    assert!(out.contains(".merge(Self::__fast_http_group_admin(self)?)"));
}

#[test]
fn controller_struct_without_a_path_is_the_root() {
    let out = expand(
        quote!(groups(all)),
        quote!(
            struct Api;
        ),
    )
    .expect("expands");

    assert!(out.contains("constPATH:&'staticstr=\"/\";"));
}

#[test]
fn controller_struct_keeps_its_generics() {
    let out = expand(
        quote!(groups(all)),
        quote!(
            struct Api<T: Send>(T);
        ),
    )
    .expect("expands");

    assert!(out.contains("impl<T:Send>::fast_http::router::ControllerforApi<T>"));
}

#[test]
fn controller_struct_needs_its_groups_listed_once() {
    assert!(
        error(
            quote!("/a"),
            quote!(
                struct A;
            )
        )
        .contains("groups(name, ..)")
    );
    assert!(
        error(
            quote!(groups(x, x)),
            quote!(
                struct A;
            )
        )
        .contains("listed twice")
    );
    assert!(
        error(
            quote!(group = x),
            quote!(
                struct A;
            )
        )
        .contains("belongs on an impl block")
    );
}

#[test]
fn controller_struct_refuses_a_malformed_path() {
    assert!(
        error(
            quote!("library", groups(x)),
            quote!(
                struct A;
            )
        )
        .contains("must start with `/`")
    );
}
