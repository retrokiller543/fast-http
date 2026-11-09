use super::*;

#[test]
fn alpn_ids_round_trip() {
    for alpn in Alpn::ALL {
        assert_eq!(Alpn::from_id(alpn.id()), Some(alpn));
    }

    assert_eq!(Alpn::from_id(b"h3"), None);
}
