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

fn position(haystack: &str, needle: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is missing from {haystack}"))
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
fn expand_unguarded_method_is_bound_without_a_dispatcher() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get("/a")]
                async fn a(&self, x: Path<u64>, parts: &Parts) -> String { String::new() }
            }
        },
    );

    assert!(
        out.contains(
            "::fast_http::router::bind(::std::sync::Arc::clone(&self),Self::__fast_http_a)"
        )
    );
    assert!(out.contains("__a0:Path<u64>,__a1:&Parts"));
    assert!(out.contains("this.a(__a0,__a1).await"));
    assert!(!out.contains("Endpointfor"));
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
fn expand_guarded_variants_become_one_dispatcher() {
    let out = expand(
        quote!("/chat"),
        quote! {
            impl Api {
                #[post(when(q.stream))]
                async fn stream(&self, Query(q): Query<Params>) -> String { String::new() }

                #[post]
                async fn plain(&self) -> String { String::new() }
            }
        },
    );

    assert_eq!(out.matches("Route::new(").count(), 1);
    assert_eq!(out.matches(".endpoint(").count(), 1);
    assert!(out.contains("::fast_http::core::Endpointfor__FastHttp_Api_stream_POST"));
    assert!(out.contains("::fast_http::core::http::Method::POST"));
    assert!(!out.contains("bind("));
}

#[test]
fn expand_emits_the_guard_exactly_as_written() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[post(when(q.stream == Some(true) && !q.debug))]
                async fn stream(&self, Query(q): Query<Params>) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("if!(q.stream==Some(true)&&!q.debug){break'__v0;}"));
    assert!(out.contains("letQuery(q)=match&__s0"));
}

#[test]
fn expand_extracts_for_the_guard_before_the_remaining_arguments() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[post(when(headers.contains_key("x")))]
                async fn fast(&self, headers: &HeaderMap, body: Body<Req>) -> String { String::new() }
            }
        },
    );
    let head = position(&out, "extract_head::<&HeaderMap>");
    let guard = position(&out, "if!(headers.contains_key(\"x\"))");
    let body = position(&out, "extract_last::<Body<Req>");

    assert!(head < guard);
    assert!(guard < body);
}

#[test]
fn expand_guard_on_the_last_argument_extracts_only_what_it_needs_first() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[post(when(body.value.fast))]
                async fn fast(&self, headers: &HeaderMap, body: Body<Req>) -> String { String::new() }
            }
        },
    );
    let body = position(&out, "extract_last::<Body<Req>");
    let guard = position(&out, "if!(body.value.fast)");
    let head = position(&out, "extract_head::<&HeaderMap>");

    assert!(body < guard);
    assert!(guard < head);
}

#[test]
fn expand_variants_taking_the_same_type_share_one_slot() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[post("/ask", when(body.fast))]
                async fn fast(&self, body: Body<Req>) -> String { String::new() }

                #[post("/ask")]
                async fn any(&self, body: Body<Req>) -> String { String::new() }
            }
        },
    );

    assert_eq!(out.matches("=::std::option::Option::None;").count(), 1);
    assert!(out.contains("let__multi=false;"));
}

#[test]
fn expand_variants_taking_different_body_types_decide_buffering_at_runtime() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[post("/two", when(first.ok))]
                async fn a(&self, first: A) -> String { String::new() }

                #[post("/two")]
                async fn b(&self, second: B) -> String { String::new() }
            }
        },
    );

    assert_eq!(out.matches("=::std::option::Option::None;").count(), 2);
    assert!(out.contains("uses_body::<A,_>()"));
    assert!(out.contains("uses_body::<B,_>()"));
}

#[test]
fn expand_single_body_type_never_buffers() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[post(when(body.fast))]
                async fn a(&self, body: A) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("let__multi=false;"));
}

#[test]
fn expand_last_guarded_variant_renders_a_guard_miss() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get(when(q.x))]
                async fn a(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("dispatch::guard_miss(__parts,__handler).await"));
}

#[test]
fn expand_unguarded_last_variant_has_no_guard_miss() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get(when(q.x))]
                async fn a(&self, q: QueryParams<'_>) -> String { String::new() }

                #[get]
                async fn b(&self) -> String { String::new() }
            }
        },
    );

    assert!(!out.contains("guard_miss"));
}

#[test]
fn expand_params_read_by_guards_are_allowed_to_be_unused() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get(when(q.x))]
                async fn a(&self, q: QueryParams<'_>, other: u8) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("#[allow(unused_variables)]q:QueryParams<'_>"));
    assert!(!out.contains("#[allow(unused_variables)]other"));
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
fn expand_omitted_path_equals_an_empty_one() {
    let omitted = expand(
        quote!("/chat"),
        quote! {
            impl Api {
                #[post(when(q.x))]
                async fn a(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );
    let explicit = expand(
        quote!("/chat"),
        quote! {
            impl Api {
                #[post("", when(q.x))]
                async fn a(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert_eq!(omitted, explicit);
    assert!(omitted.contains("Route::new(\"/chat\")?"));
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
                #[get(when(q.x))]
                async fn a(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("my::http::router::Router::new()"));
    assert!(out.contains("my::http::core::Endpointfor"));
    assert!(!out.contains("::fast_http"));
}

#[test]
fn expand_generic_controller_keeps_its_generics_on_the_dispatcher() {
    let out = expand(
        quote!(),
        quote! {
            impl<T: Send> Api<T> {
                #[get(when(q.x))]
                async fn a(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("struct__FastHttp_Api_a_GET<T:Send>"));
    assert!(out.contains("this:::std::sync::Arc<Api<T>>"));
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
    assert_eq!(out.matches(".endpoint(").count(), 2);
}

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
fn expand_builds_the_reserved_views_inside_the_guard_that_names_them() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get(when(headers.contains_key("x"), query.contains("y")))]
                async fn a(&self) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("letheaders=&__parts.headers;if!(headers.contains_key(\"x\"))"));
    assert!(out.contains(
        "letquery=::fast_http::core::QueryParams::of(__parts);if!(query.contains(\"y\"))"
    ));
}

#[test]
fn expand_builds_no_view_for_a_guard_that_does_not_name_it() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get(when(q.x))]
                async fn a(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert!(!out.contains("letheaders="));
    assert!(!out.contains("letquery="));
}

#[test]
fn expand_a_parameter_with_the_reserved_name_takes_precedence() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get(when(query.contains("y")))]
                async fn a(&self, query: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert!(!out.contains("letquery=::fast_http::core::QueryParams::of"));
    assert!(out.contains("letquery=match&__s0"));
}

#[test]
fn expand_reserved_views_are_decided_per_variant() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get(when(query.contains("y")))]
                async fn a(&self) -> String { String::new() }

                #[get(when(query.contains("z")))]
                async fn b(&self, query: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert_eq!(
        out.matches("letquery=::fast_http::core::QueryParams::of(__parts);")
            .count(),
        1
    );
    assert_eq!(out.matches("letquery=match&__s").count(), 1);
}

#[test]
fn expand_guard_without_parameters_runs_before_any_extraction() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[post(when(query.contains("fast")))]
                async fn a(&self, body: Body<Req>) -> String { String::new() }
            }
        },
    );
    let guard = position(&out, "if!(query.contains(\"fast\"))");
    let body = position(&out, "extract_last::<Body<Req>");

    assert!(guard < body);
}

#[test]
fn expand_keeps_the_generated_locals_out_of_the_users_namespace() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get(when(q.x))]
                async fn a(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("__body:B"));
    assert!(out.contains("__handler:&H"));
    assert!(!out.contains(",body:B"));
}
