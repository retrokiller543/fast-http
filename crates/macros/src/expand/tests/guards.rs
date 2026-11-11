use super::*;

#[test]
fn expand_unguarded_method_gets_a_generated_endpoint_that_never_declines() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get("/a")]
                async fn a(&self, x: Path<u64>, parts: &Parts) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("Endpointfor__FastHttp_Api_a_GET_0"));
    assert!(out.contains("__this.a(__a0,__a1).await"));
    assert!(!out.contains("router::bind"));
    assert!(!out.contains("Ok(::std::option::Option::None)}.await"));
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
    assert!(out.contains("letQuery(q)=__rt::fill(&mut__s0"));
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

    assert_eq!(out.matches("=__rt::slot::<").count(), 1);
}

#[test]
fn expand_last_guarded_variant_declines_the_request() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get(when(q.x))]
                async fn a(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("Ok(::std::option::Option::None)}.await"));
}

#[test]
fn expand_unguarded_last_variant_never_declines() {
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

    assert!(!out.contains("Ok(::std::option::Option::None)}.await"));
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
    assert!(out.contains("letquery={use"));
    assert!(out.contains("(&__rt::Guarded(__rt::fill(&mut__s0"));
}

#[test]
fn expand_plain_parameter_is_seen_by_the_guard_through_its_view() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[post(when(matches!(job, Job::Fast)))]
                async fn a(&self, job: Body<Job>) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("letjob={use__rt::{ViewInneras_,ViewSelfas_};"));
    assert!(out.contains("(&__rt::Guarded(__rt::fill(&mut__s0"));
    assert!(out.contains(".view()"));
}

#[test]
fn expand_destructured_parameter_is_bound_as_written() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get(when(q.x))]
                async fn a(&self, Query(q): Query<Params>) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("letQuery(q)=__rt::fill(&mut__s0"));
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
    assert_eq!(out.matches("letquery={use").count(), 1);
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

    assert!(out.contains("__body:__B"));
    assert!(out.contains("__handler:&__H"));
    assert!(out.contains("call<'__r,__B:"));
}

#[test]
fn expand_annotates_each_slot_with_the_declared_type() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[post(when(body.fast))]
                async fn a(&self, body: Body<Req>) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("letmut__s0=__rt::slot::<Body<Req>>();"));
}

#[test]
fn expand_half_typed_guard_still_expands_everything_and_reports_the_error() {
    let out = expand(
        quote!("/chat"),
        quote! {
            impl Api {
                #[post(when(params.))]
                async fn stream(&self, Query(params): Query<Params>) -> String { String::new() }

                #[post(when(q.fast))]
                async fn other(&self, q: QueryParams<'_>) -> String { String::new() }

                #[post]
                async fn plain(&self) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("compile_error"));
    assert!(out.contains("letQuery(params)=__rt::fill(&mut__s0"));
    assert!(out.contains("if!(params.__incomplete)"));
    assert!(out.contains("if!(q.fast)"));
    assert!(out.contains("pubfnroutes("));
}

#[test]
fn expand_guard_ending_in_an_operator_is_completed_with_false() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get(when(q.a &&))]
                async fn a(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("if!(q.a&&false)"));
    assert!(out.contains("compile_error"));
}

#[test]
fn expand_guard_that_cannot_be_repaired_becomes_false_and_keeps_the_others() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get(when(let, q.ok))]
                async fn a(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("if!(false)"));
    assert!(out.contains("if!(q.ok)"));
    assert!(out.contains("compile_error"));
}

#[test]
fn expand_guards_with_commas_inside_closures_are_not_split() {
    let out = expand(
        quote!(),
        quote! {
            impl Api {
                #[get(when(items.iter().fold(0, |sum, n| sum + n) > 3, flag))]
                async fn a(&self, items: Items, flag: Flag) -> String { String::new() }
            }
        },
    );

    assert!(!out.contains("compile_error"));
    assert!(out.contains("fold(0,|sum,n|sum+n)>3"));
}
