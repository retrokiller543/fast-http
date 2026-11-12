use bytes::Bytes;
use fast_http_core::{
    BoxError, FromRequest, IntoOutcome, MediaType, Outcome, ReqBody, Response, ResponseBody,
};
use http::{HeaderValue, header::CONTENT_TYPE, request::Parts};

use crate::{
    Body, Decode, Encode, Format, FormatError,
    wrapper::{Wrapper, transparent},
};

/// Defines one serde format as a single type with two roles.
///
/// Bare, as in `Body<T, (Json, Yaml)>`, it is the format marker of a set: `Json` is `Json<()>`.
/// Wrapping a value, `Json<T>` is an extractor that accepts only that format and a response that
/// is always that format, whatever the client's `Accept` says.
macro_rules! serde_format {
    (
        $(#[$meta:meta])*
        $name:ident, $mime:literal,
        |$value:ident| $encode:expr,
        |$input:ident| $decode:expr
    ) => {
        $(#[$meta])*
        ///
        /// Bare, it names the format in a set, such as `Body<T, (Self, ..)>`. Wrapping a value, it
        /// is an extractor that accepts only this format and a response that is always this
        /// format; the value is reached through `Deref` or the public field.
        pub struct $name<T = ()>(pub T);

        impl<T> Wrapper for $name<T> {
            type Inner = T;

            fn get(&self) -> &T {
                &self.0
            }

            fn get_mut(&mut self) -> &mut T {
                &mut self.0
            }

            fn unwrap(self) -> T {
                self.0
            }

            fn wrap(value: T) -> Self {
                Self(value)
            }
        }

        transparent!(value $name [T] [T]);

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

        impl<T: serde::de::DeserializeOwned + Send> FromRequest for $name<T> {
            type Rejection = FormatError;

            async fn from_request<B: ReqBody>(
                parts: &Parts,
                body: B,
            ) -> Result<Self, Self::Rejection> {
                let body = Body::<T, ($name,)>::from_request(parts, body).await?;

                Ok(Self(body.into_inner()))
            }
        }

        impl<T: serde::Serialize> IntoOutcome for $name<T> {
            fn into_outcome(self, parts: &Parts) -> Outcome {
                encoded::<$name, T>(&self.0, parts)
            }
        }
    };
}

/// Answers with `value` encoded by `F`, labelled with `F`'s media type.
fn encoded<F: Encode<T>, T>(value: &T, parts: &Parts) -> Outcome {
    let bytes = F::encode(value, parts)?;
    let mut response = Response::new(ResponseBody::Bytes(bytes));

    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static(F::MEDIA_TYPE.essence()),
    );

    Ok(response)
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
