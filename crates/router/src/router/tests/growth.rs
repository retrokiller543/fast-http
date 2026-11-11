use super::*;

macro_rules! numbered {
    ($router:expr; $($n:literal)+) => {
        $router$(.route(
            Route::new(concat!("/r", $n))
                .expect("valid pattern")
                .get(|| async { concat!("route ", $n) }),
        ))+
    };
}

async fn any() -> &'static str {
    "any"
}

fn text_of(router: &impl Routes, uri: &str) -> String {
    let (status, _, response) = send(router, Method::GET, uri);
    let mut cx = Context::from_waker(Waker::noop());
    let Poll::Ready(text) = pin!(body_text(response)).poll(&mut cx) else {
        panic!("a full body is ready immediately");
    };

    assert_eq!(status, StatusCode::OK, "{uri}");

    text
}

#[test]
fn router_past_sixteen_routes_dispatches_across_level_carries() {
    let router = numbered!(Router::new();
        0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19
        20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39);

    for n in [0, 15, 16, 17, 31, 32, 39] {
        assert_eq!(text_of(&router, &format!("/r{n}")), format!("route {n}"));
    }

    let (status, ..) = send(&router, Method::GET, "/r40");

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[test]
fn router_prefers_the_more_specific_route_across_level_carries() {
    let head = numbered!(Router::new();
        0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19);
    let router =
        numbered!(head.route(Route::new("/{name}").expect("valid pattern").get(any)); 20 21);

    assert_eq!(text_of(&router, "/r5"), "route 5");
    assert_eq!(text_of(&router, "/r19"), "route 19");
    assert_eq!(text_of(&router, "/r21"), "route 21");
    assert_eq!(text_of(&router, "/other"), "any");
}

#[test]
fn router_applies_route_layers_after_a_carry() {
    let head = numbered!(Router::new();
        0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);
    let router = head.route(
        Route::new("/closed")
            .expect("valid pattern")
            .get(health)
            .layer(Deny),
    );

    let (status, ..) = send(&router, Method::GET, "/closed");

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(text_of(&router, "/r16"), "route 16");
}

#[test]
fn empty_router_returns_404() {
    let (status, ..) = send(&Router::new(), Method::GET, "/");

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[test]
fn merge_prefers_the_more_specific_route_whatever_the_group_order() {
    let users = numbered!(Router::new(); 0 1 2);
    let fallback = Router::new().route(Route::new("/{name}").expect("valid pattern").get(any));
    let late = numbered!(Router::new(); 3);
    let router = numbered!(Router::new(); 10 11)
        .merge(users)
        .merge(fallback)
        .merge(late);

    assert_eq!(text_of(&router, "/r10"), "route 10");
    assert_eq!(text_of(&router, "/r2"), "route 2");
    assert_eq!(text_of(&router, "/r3"), "route 3");
    assert_eq!(text_of(&router, "/other"), "any");
}

#[test]
#[should_panic(expected = "matches the same paths as `/{name}`")]
fn route_with_the_shape_of_an_earlier_one_is_refused() {
    let _ = Router::new()
        .route(Route::new("/{name}").expect("valid pattern").get(any))
        .route(Route::new("/{other}").expect("valid pattern").get(any));
}

#[test]
#[should_panic(expected = "matches the same paths as `/r0`")]
fn merged_route_with_the_shape_of_an_existing_one_is_refused() {
    let _ = numbered!(Router::new(); 0).merge(numbered!(Router::new(); 0));
}
