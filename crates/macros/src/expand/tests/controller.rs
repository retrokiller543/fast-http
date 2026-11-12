use super::*;

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
    assert!(out.contains("Route::new(\"/users/{id}\")?"));
}

#[test]
fn expand_method_without_receiver_is_called_by_path() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get("/health")]
                async fn health() -> &'static str { "ok" }
            }
        },
    );

    assert!(out.contains("<Api>::health()"));
    assert!(!out.contains("__fast_http_health"));
}

#[test]
fn expand_variants_with_different_last_types_assert_a_single_body_type_at_compile_time() {
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

    assert_eq!(out.matches("=__rt::slot::<").count(), 2);
    assert!(out.contains("const_:()=::std::assert!(0_usize+"));
    assert!(out.contains("uses_body::<A,_>()"));
    assert!(out.contains("uses_body::<B,_>()"));
    assert!(out.contains("mustalltakethesamebodytype"));
}

#[test]
fn expand_a_single_last_type_needs_no_body_assertion() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[post("/ask", when(body.fast))]
                async fn fast(&self, body: A) -> String { String::new() }

                #[post("/ask")]
                async fn any(&self, body: A) -> String { String::new() }
            }
        },
    );

    assert!(!out.contains("assert!"));
    assert!(!out.contains("<=1"));
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
    assert_eq!(
        out.matches("Endpointfor__FastHttp_Api_both_GET_").count(),
        2
    );
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

    assert!(out.contains("struct__FastHttp_Api_a_GET_0<T:Send>"));
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
fn expand_named_lifetimes_are_elided_in_generated_argument_types() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get("/{n}", when(n.0.len() > 1))]
                async fn a<'a>(&self, n: Path<&'a str>) -> &'a str { n.0 }

                #[get("/{n}")]
                async fn b<'a>(&self, n: Path<&'a str>) -> &'a str { n.0 }
            }
        },
    );

    assert!(out.contains("slot::<Path<&'_str>>"));
}

#[test]
fn expand_generic_controller_checks_the_body_rule_after_monomorphization() {
    let out = expand(
        quote!(),
        quote! {
            impl<T: Send> Api<T> {
                #[post("/two", when(first.ok))]
                async fn a(&self, first: A) -> String { String::new() }

                #[post("/two")]
                async fn b(&self, second: B) -> String { String::new() }
            }
        },
    );

    assert!(!out.contains("const_:()="));
    assert!(out.contains("const{::std::assert!(0_usize+"));
}

#[test]
fn expand_handler_cfg_gates_its_endpoint_and_route() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[cfg(feature = "admin")]
                #[get("/admin")]
                async fn admin(&self) -> String { String::new() }

                #[get("/open")]
                async fn open(&self) -> String { String::new() }
            }
        },
    );
    let gate = "#[cfg(all(feature=\"admin\"))]";

    assert_eq!(out.matches(gate).count(), 3, "{out}");
    assert!(out.contains("#[cfg(any(all(feature=\"admin\")))]let__router="));
    assert!(out.contains(&format!("{gate}let__route=__route.endpoint(")));
}

#[test]
fn expand_ungated_handler_on_a_route_keeps_the_route_ungated() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[cfg(feature = "admin")]
                #[delete("/item")]
                async fn remove(&self) -> String { String::new() }

                #[get("/item")]
                async fn show(&self) -> String { String::new() }
            }
        },
    );

    assert!(!out.contains("cfg(any("));
    assert!(out.contains("#[cfg(all(feature=\"admin\"))]let__route=__route.endpoint("));
}

#[test]
fn expand_guard_on_the_body_reports_that_guards_read_it() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[post("/ask", when(body.fast))]
                async fn fast(&self, body: A) -> String { String::new() }
            }
        },
    );

    assert!(out.contains(
        "fnguards_read_body(&self)->bool{false||::fast_http::router::dispatch::uses_body::<A,_>()}"
    ));
    assert!(out.contains("Attempt::Missed"));
}

#[test]
fn expand_guard_on_the_head_does_not_claim_to_read_the_body() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[post("/ask", when(q.fast))]
                async fn fast(&self, q: Q, body: A) -> String { String::new() }
            }
        },
    );

    assert!(!out.contains("guards_read_body"));
}

#[test]
fn expand_cfg_attr_that_applies_a_cfg_gates_like_a_cfg() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[cfg_attr(not(feature = "foo"), cfg(any()))]
                #[get("/foo")]
                async fn foo(&self) -> String { String::new() }
            }
        },
    );

    assert!(
        out.contains(
            "#[cfg(all(any(not(not(feature=\"foo\")),all(any()))))]let__route=__route.endpoint("
        ),
        "{out}"
    );
}
