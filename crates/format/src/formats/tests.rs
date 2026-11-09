use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct User {
    name: String,
    age: u8,
}

fn user() -> User {
    User {
        name: "ada".to_owned(),
        age: 36,
    }
}

fn roundtrip<F: Encode<User> + Decode<User>>() {
    let parts = http::Request::new(()).into_parts().0;

    let bytes = F::encode(&user(), &parts).expect("user should encode");
    let decoded = F::decode(&bytes).expect("encoded user should decode");

    assert_eq!(decoded, user());
}

#[cfg(feature = "json")]
#[test]
fn json_roundtrip_preserves_value() {
    roundtrip::<Json>();
}

#[cfg(feature = "form")]
#[test]
fn form_roundtrip_preserves_value() {
    roundtrip::<Form>();
}

#[cfg(feature = "yaml")]
#[test]
fn yaml_roundtrip_preserves_value() {
    roundtrip::<Yaml>();
}

#[cfg(feature = "toml")]
#[test]
fn toml_roundtrip_preserves_value() {
    roundtrip::<Toml>();
}

#[cfg(feature = "cbor")]
#[test]
fn cbor_roundtrip_preserves_value() {
    roundtrip::<Cbor>();
}

#[cfg(feature = "msgpack")]
#[test]
fn msgpack_roundtrip_preserves_value() {
    roundtrip::<MessagePack>();
}

#[cfg(feature = "xml")]
#[test]
fn xml_roundtrip_preserves_value() {
    roundtrip::<Xml>();
}
