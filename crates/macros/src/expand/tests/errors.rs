use super::*;

#[test]
fn error_trait_impl_is_rejected() {
    assert!(error(quote!(), quote!(impl Trait for Api {})).contains("inherent impl"));
}

#[test]
fn error_sync_method_is_rejected() {
    let message = error(
        quote!(),
        quote! { impl Api { #[get("/a")] fn a(&self) -> String { String::new() } } },
    );

    assert!(message.contains("must be `async fn`"));
}

#[test]
fn error_generic_method_is_rejected() {
    let message = error(
        quote!(),
        quote! { impl Api { #[get("/a")] async fn a<T>(&self) -> String { String::new() } } },
    );

    assert!(message.contains("cannot have type or const parameters"));
}

#[test]
fn error_mutable_receiver_is_rejected() {
    let message = error(
        quote!(),
        quote! { impl Api { #[get("/a")] async fn a(&mut self) -> String { String::new() } } },
    );

    assert!(message.contains("`&self` or no receiver"));
}

#[test]
fn error_by_value_receiver_is_rejected() {
    let message = error(
        quote!(),
        quote! { impl Api { #[get("/a")] async fn a(self) -> String { String::new() } } },
    );

    assert!(message.contains("`&self` or no receiver"));
}

#[test]
fn error_guard_using_self_is_rejected() {
    let message = error(
        quote!(),
        quote! { impl Api { #[get(when(self.enabled))] async fn a(&self) -> String { String::new() } } },
    );

    assert!(message.contains("cannot use `self`"));
}

#[test]
fn error_duplicate_endpoint_is_rejected() {
    let message = error(
        quote!(),
        quote! {
            impl Api {
                #[get("/a", when(q.x))]
                async fn one(&self, q: QueryParams<'_>) -> String { String::new() }

                #[get("/a", when(q.x))]
                async fn two(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert!(message.contains("repeats an earlier endpoint"));
}

#[test]
fn error_variant_after_an_unguarded_one_is_unreachable() {
    let message = error(
        quote!(),
        quote! {
            impl Api {
                #[get("/a")]
                async fn plain(&self) -> String { String::new() }

                #[get("/a", when(q.x))]
                async fn guarded(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert!(message.contains("unreachable"));
}

#[test]
fn error_bad_route_path_points_at_the_path() {
    let message = error(
        quote!(),
        quote! { impl Api { #[get("a")] async fn a(&self) -> String { String::new() } } },
    );

    assert!(message.contains("must start with `/`"));
}

#[test]
fn error_handlers_sharing_an_endpoint_with_different_cfgs_are_rejected() {
    let message = error(
        quote!(),
        quote! {
            impl Api {
                #[cfg(feature = "fast")]
                #[post("/ask", when(q.fast))]
                async fn fast(&self, q: Q) -> String { String::new() }

                #[post("/ask")]
                async fn slow(&self) -> String { String::new() }
            }
        },
    );

    assert!(
        message.contains("need the same #[cfg(..)] attributes"),
        "{message}"
    );
}

#[test]
fn error_crate_on_a_route_inside_a_controller_is_rejected() {
    let message = error(
        quote!(),
        quote! {
            impl Api {
                #[get("/a", crate = ::web)]
                async fn a(&self) -> String { String::new() }
            }
        },
    );

    assert!(
        message.contains("goes on the #[controller] or #[routes] attribute"),
        "{message}"
    );
}
