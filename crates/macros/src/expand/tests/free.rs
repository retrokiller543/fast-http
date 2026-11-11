use super::*;

#[test]
fn routes_module_gets_a_stateless_routes_function() {
    let out = expand_routes(
        quote!("/items"),
        quote! {
            mod items {
                #[get("/{id}", when(*id == 0))]
                async fn zero(Path(id): Path<u64>) -> String { String::new() }

                #[get("/{id}")]
                async fn show(Path(id): Path<NonZeroU64>) -> String { String::new() }
            }
        },
    );

    assert!(out.contains("pubfnroutes()->::std::result::Result<"));
    assert!(out.contains("Route::new(\"/items/{id}\")?"));
    assert!(out.contains("structFree_") || out.contains("struct__FastHttp_Free_zero_GET"));
    assert!(out.contains("<NonZeroU64>") && out.contains("zero(__a0)"));
    assert!(!out.contains("Arc"));
}

#[test]
fn routes_module_keeps_the_generated_endpoint_inside_the_module() {
    let out = expand_routes(
        quote!(),
        quote! {
            mod items {
                #[get(when(headers.contains_key("x")))]
                async fn a() -> String { String::new() }

                #[get]
                async fn b() -> String { String::new() }
            }
        },
    );
    let module_end = out.rfind('}').expect("module end");
    let endpoint = position(&out, "Endpointfor__FastHttp_Free_a_GET_0");

    assert!(endpoint < module_end);
    assert!(out.contains("__FastHttp_Free_a_GET_0)"));
}

#[test]
fn routes_module_unguarded_function_gets_a_generated_endpoint() {
    let out = expand_routes(
        quote!(),
        quote! {
            mod items {
                #[get("/health")]
                async fn health() -> &'static str { "ok" }
            }
        },
    );

    assert!(out.contains("Endpointfor__FastHttp_Free_health_GET_0"));
}

#[test]
fn routes_module_must_be_inline() {
    let message = routes(
        quote!(),
        quote!(
            mod items;
        ),
    )
    .map(drop)
    .expect_err("error")
    .to_string();

    assert!(message.contains("inline module"));
}

#[test]
fn routes_module_function_with_a_receiver_is_rejected() {
    let message = routes(
        quote!(),
        quote! { mod items { #[get("/a")] async fn a(&self) -> String { String::new() } } },
    )
    .map(drop)
    .expect_err("error")
    .to_string();

    assert!(message.contains("has no `self`"));
}

#[test]
fn standalone_function_becomes_a_unit_struct_that_is_its_own_route() {
    let out = expand_standalone(
        HttpMethod::Get,
        quote!("/deep", when(query.contains("full"))),
        quote! {
            pub async fn deep(Path(id): Path<u64>) -> String { String::new() }
        },
    );

    assert!(out.contains("pubstructdeep;"));
    assert!(out.contains("asyncfn__fast_http_handler_deep("));
    assert!(out.contains("::fast_http::core::Endpointfordeep"));
    assert!(out.contains("pubconstPATH:&'staticstr=\"/deep\";"));
    assert!(out.contains("::fast_http::router::IntoRoutefordeep"));
    assert!(out.contains("attributed(Self::PATH).endpoint(self)"));
    assert!(out.contains("__fast_http_handler_deep(__a0)"));
}

#[test]
fn standalone_function_keeps_a_private_struct_private() {
    let out = expand_standalone(
        HttpMethod::Get,
        quote!("/a"),
        quote! { async fn a() -> String { String::new() } },
    );

    assert!(out.contains("structa;"));
    assert!(!out.contains("pubstructa"));
}

#[test]
fn standalone_without_a_path_is_the_root() {
    let out = expand_standalone(
        HttpMethod::Post,
        quote!(when(query.contains("x"))),
        quote! { async fn a() -> String { String::new() } },
    );

    assert!(out.contains("constPATH:&'staticstr=\"/\";"));
    assert!(out.contains("::fast_http::core::http::Method::POST"));
}

#[test]
fn standalone_rejects_a_receiver_and_a_sync_function() {
    let receiver = standalone(
        HttpMethod::Get,
        quote!("/a"),
        quote! { async fn a(&self) -> String { String::new() } },
    )
    .map(drop)
    .expect_err("error")
    .to_string();
    let sync = standalone(
        HttpMethod::Get,
        quote!("/a"),
        quote! { fn a() -> String { String::new() } },
    )
    .map(drop)
    .expect_err("error")
    .to_string();

    assert!(receiver.contains("has no `self`"));
    assert!(sync.contains("must be `async fn`"));
}

#[test]
fn route_general_form_inside_a_controller_matches_the_shorthand() {
    let general = expand(
        quote!("/chat"),
        quote! {
            impl Api {
                #[route(POST, when(q.x))]
                async fn a(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );
    let shorthand = expand(
        quote!("/chat"),
        quote! {
            impl Api {
                #[post(when(q.x))]
                async fn a(&self, q: QueryParams<'_>) -> String { String::new() }
            }
        },
    );

    assert_eq!(general, shorthand);
}

#[test]
fn route_general_form_on_a_lone_function_matches_the_shorthand() {
    let general = standalone_route(
        quote!(GET, "/deep", when(query.contains("full"))),
        quote! { async fn deep() -> String { String::new() } },
    )
    .expect("expands")
    .to_string();
    let shorthand = standalone(
        HttpMethod::Get,
        quote!("/deep", when(query.contains("full"))),
        quote! { async fn deep() -> String { String::new() } },
    )
    .expect("expands")
    .to_string();

    assert_eq!(general, shorthand);
}

#[test]
fn standalone_crate_override_changes_every_path() {
    let out = expand_standalone(
        HttpMethod::Get,
        quote!("/health", crate = ::web),
        quote!(
            async fn health() -> &'static str {
                "ok"
            }
        ),
    );

    assert!(out.contains("::web::router::IntoRoute"));
    assert!(!out.contains("::fast_http"), "{out}");
}

#[test]
fn standalone_cfg_gates_every_generated_item() {
    let out = expand_standalone(
        HttpMethod::Get,
        quote!("/admin"),
        quote! {
            #[cfg(feature = "admin")]
            async fn admin() -> &'static str { "ok" }
        },
    );

    assert_eq!(out.matches("#[cfg(feature=\"admin\")]").count(), 6, "{out}");
}
