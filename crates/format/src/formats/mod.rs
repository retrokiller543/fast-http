use bytes::Bytes;
use fast_http_core::{BoxError, MediaType};
use http::request::Parts;

use crate::{Decode, Encode, Format, FormatError};

macro_rules! serde_format {
    (
        $(#[$meta:meta])*
        $name:ident, $mime:literal,
        |$value:ident| $encode:expr,
        |$input:ident| $decode:expr
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default)]
        pub struct $name;

        impl Format for $name {
            const MEDIA_TYPE: MediaType = MediaType::new($mime);
        }

        impl<T: serde::Serialize> Encode<T> for $name {
            fn encode($value: &T, _parts: &Parts) -> Result<Bytes, FormatError> {
                let encoded: Result<Vec<u8>, BoxError> = $encode;

                encoded.map(Bytes::from).map_err(FormatError::Encode)
            }
        }

        impl<T: serde::de::DeserializeOwned> Decode<T> for $name {
            fn decode($input: &[u8]) -> Result<T, FormatError> {
                let decoded: Result<T, BoxError> = $decode;

                decoded.map_err(FormatError::Decode)
            }
        }
    };
}

#[cfg(feature = "json")]
serde_format!(
    /// `application/json`.
    Json, "application/json",
    |value| serde_json::to_vec(value).map_err(Into::into),
    |input| serde_json::from_slice(input).map_err(Into::into)
);

#[cfg(feature = "form")]
serde_format!(
    /// `application/x-www-form-urlencoded`; multipart is a separate, field-oriented extractor.
    Form, "application/x-www-form-urlencoded",
    |value| serde_urlencoded::to_string(value)
        .map(String::into_bytes)
        .map_err(Into::into),
    |input| serde_urlencoded::from_bytes(input).map_err(Into::into)
);

#[cfg(feature = "yaml")]
serde_format!(
    /// `application/yaml`.
    Yaml, "application/yaml",
    |value| serde_norway::to_string(value)
        .map(String::into_bytes)
        .map_err(Into::into),
    |input| serde_norway::from_slice(input).map_err(Into::into)
);

#[cfg(feature = "toml")]
serde_format!(
    /// `application/toml`.
    Toml, "application/toml",
    |value| toml::to_string(value)
        .map(String::into_bytes)
        .map_err(Into::into),
    |input| std::str::from_utf8(input)
        .map_err(BoxError::from)
        .and_then(|text| toml::from_str(text).map_err(Into::into))
);

#[cfg(feature = "cbor")]
serde_format!(
    /// `application/cbor`.
    Cbor, "application/cbor",
    |value| {
        let mut buffer = Vec::new();

        ciborium::ser::into_writer(value, &mut buffer)
            .map(|()| buffer)
            .map_err(Into::into)
    },
    |input| ciborium::de::from_reader(input).map_err(Into::into)
);

#[cfg(feature = "msgpack")]
serde_format!(
    /// `application/msgpack`, with named struct fields.
    MessagePack, "application/msgpack",
    |value| rmp_serde::to_vec_named(value).map_err(Into::into),
    |input| rmp_serde::from_slice(input).map_err(Into::into)
);

#[cfg(feature = "xml")]
serde_format!(
    /// `application/xml`.
    Xml, "application/xml",
    |value| quick_xml::se::to_string(value)
        .map(String::into_bytes)
        .map_err(Into::into),
    |input| quick_xml::de::from_reader(input).map_err(Into::into)
);

#[cfg(test)]
mod tests;
