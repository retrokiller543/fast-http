use quote::quote;

use super::*;

fn expand(args: TokenStream, item: TokenStream) -> String {
    controller(args, item)
        .expect("expands")
        .to_string()
        .replace(' ', "")
}

fn error(args: TokenStream, item: TokenStream) -> String {
    controller(args, item)
        .map(drop)
        .expect_err("error")
        .to_string()
}

#[test]
fn expand_builds_a_route_with_the_joined_pattern() {
    let out = expand(
        quote!("/users"),
        quote! {
            impl Api {
                #[get("/{id}")]
                async fn show(&self, id: Path<u64>) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("Route::new(\"/users/{id}\")?"));
    assert!(out.contains("::fast_http::router::get("));
}

#[test]
fn expand_binds_the_arc_to_receiver_methods() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get("/a")]
                async fn a(&self) -> String { String::new() }
            }
        },
    );

    assert!(
        out.contains(
            "::fast_http::router::bind(::std::sync::Arc::clone(&self),Self::__fast_http_a)"
        )
    );
    assert!(out.contains("asyncfn__fast_http_a(this:::std::sync::Arc<Self>)"));
}

#[test]
fn expand_wrapper_forwards_every_argument() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[post("/a")]
                async fn a(&self, x: Path<u64>, parts: &Parts) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("__a0:Path<u64>,__a1:&Parts"));
    assert!(out.contains("this.a(__a0,__a1).await"));
}

#[test]
fn expand_method_without_receiver_is_used_directly() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get("/health")]
                async fn health() -> &'static str { "ok" }
            }
        },
    );

    assert!(out.contains("get(Self::health)"));
    assert!(!out.contains("__fast_http_health"));
}

#[test]
fn expand_groups_endpoints_sharing_a_path_into_one_route() {
    let out = expand(
        quote!("/chat"),
        quote! {
            impl Api {
                #[post("", when(query.stream = true))]
                async fn stream(&self) -> String { String::new() }

                #[post("")]
                async fn plain(&self) -> String { String::new() }
            }
        },
    );

    assert_eq!(out.matches("Route::new(").count(), 1);
    assert_eq!(out.matches(".endpoint(").count(), 2);
    assert!(out.contains(".guard(|parts|true&&"));
}

#[test]
fn expand_omitted_path_equals_an_empty_one() {
    let omitted = expand(
        quote!("/chat"),
        quote! {
            impl Api {
                #[post]
                async fn a(&self) -> String { String::new() }

                #[get(when(query.x))]
                async fn b(&self) -> String { String::new() }
            }
        },
    );
    let explicit = expand(
        quote!("/chat"),
        quote! {
            impl Api {
                #[post("")]
                async fn a(&self) -> String { String::new() }

                #[get("", when(query.x))]
                async fn b(&self) -> String { String::new() }
            }
        },
    );

    assert_eq!(omitted, explicit);
    assert!(omitted.contains("Route::new(\"/chat\")?"));
}

#[test]
fn expand_one_method_with_two_attributes_adds_two_endpoints() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get("/a")]
                #[get("/b")]
                async fn both(&self) -> String { String::new() }
            }
        },
    );

    assert_eq!(out.matches("Route::new(").count(), 2);
    assert_eq!(out.matches("asyncfn__fast_http_both").count(), 1);
}

#[test]
fn expand_strips_route_attributes_but_keeps_others() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[doc = "docs"]
                #[get("/a")]
                async fn a(&self) -> String { String::new() }
            }
        },
    );

    assert!(!out.contains("#[get"));
    assert!(out.contains("#[doc=\"docs\"]"));
}

#[test]
fn expand_leaves_unmarked_methods_alone() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                fn helper(&self) -> u8 { 1 }
            }
        },
    );

    assert!(out.contains("fnhelper(&self)->u8"));
    assert!(!out.contains("Route::new("));
}

#[test]
fn expand_crate_override_changes_every_path() {
    let out = expand(
        quote!(crate = my::http),
        quote! {
            impl Api {
                #[get("/a")]
                async fn a(&self) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("my::http::router::Router::new()"));
    assert!(!out.contains("::fast_http"));
}

#[test]
fn error_trait_impl_is_rejected() {
    let message = error(quote!(), quote!(impl Trait for Api {}));

    assert!(message.contains("inherent impl"));
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

    assert!(message.contains("cannot be generic"));
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
fn error_duplicate_endpoint_is_rejected() {
    let message = error(
        quote!(),
        quote! {
            impl Api {
                #[get("/a", when(query.x))]
                async fn one(&self) -> String { String::new() }

                #[get("/a", when(query.x))]
                async fn two(&self) -> String { String::new() }
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

                #[get("/a", when(query.x))]
                async fn guarded(&self) -> String { String::new() }
            }
        },
    );

    assert!(message.contains("unreachable"));
}

#[test]
fn expand_other_methods_on_one_path_do_not_conflict() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get("/a")]
                async fn read(&self) -> String { String::new() }

                #[post("/a")]
                async fn write(&self) -> String { String::new() }
            }
        },
    );

    assert_eq!(out.matches("Route::new(").count(), 1);
}

#[test]
fn error_bad_route_path_points_at_the_path() {
    let message = error(
        quote!(),
        quote! { impl Api { #[get("a")] async fn a(&self) -> String { String::new() } } },
    );

    assert!(message.contains("must start with `/`"));
}
