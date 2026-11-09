use super::*;

#[test]
fn every_repeat_starts_with_the_next_framework() {
    let all = [Framework::FastHttp, Framework::Axum, Framework::Actix];

    assert_eq!(rotated(&all, 0), all);
    assert_eq!(
        rotated(&all, 1),
        [Framework::Axum, Framework::Actix, Framework::FastHttp]
    );
    assert_eq!(rotated(&all, 3), all);
}
