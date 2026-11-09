use super::*;

#[test]
fn spread_takes_median_and_range() {
    let spread = spread(&mut [3.0, 1.0, 2.0, 10.0]).expect("values");

    assert_eq!((spread.median, spread.min, spread.max), (2.5, 1.0, 10.0));
    assert!(super::spread(&mut []).is_none());
}
