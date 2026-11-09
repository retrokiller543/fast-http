use std::marker::PhantomData;

use bytes::Bytes;
use fast_http_core::MediaType;
use http::request::Parts;

use crate::{Decode, Encode, Format, FormatError};

/// An ordered, statically known list of formats; earlier entries win `Accept` ties.
pub trait FormatSet: 'static {
    /// Number of formats in the set.
    const LEN: usize;

    /// Media type at `index`, which must be below [`FormatSet::LEN`].
    fn media_type(index: usize) -> MediaType;
}

/// A [`FormatSet`] where every member can encode a `T`.
pub trait EncodeSet<T>: FormatSet {
    /// Encodes with the format at `index`.
    fn encode(index: usize, value: &T, parts: &Parts) -> Result<Bytes, FormatError>;
}

/// A [`FormatSet`] where every member can decode a `T`.
pub trait DecodeSet<T>: FormatSet {
    /// Decodes with the format at `index`.
    fn decode(index: usize, bytes: &[u8]) -> Result<T, FormatError>;
}

/// The empty set, terminating a [`Cons`] list.
#[derive(Debug, Clone, Copy)]
pub struct Nil;

/// A set made of format `H` followed by the set `Tl`, e.g. `Cons<MyFormat, Enabled>`.
pub struct Cons<H, Tl>(PhantomData<fn() -> (H, Tl)>);

impl FormatSet for Nil {
    const LEN: usize = 0;

    fn media_type(_index: usize) -> MediaType {
        MediaType::new("")
    }
}

impl<T> EncodeSet<T> for Nil {
    fn encode(_index: usize, _value: &T, _parts: &Parts) -> Result<Bytes, FormatError> {
        Err(FormatError::NotAcceptable)
    }
}

impl<T> DecodeSet<T> for Nil {
    fn decode(_index: usize, _bytes: &[u8]) -> Result<T, FormatError> {
        Err(FormatError::UnsupportedMediaType)
    }
}

impl<H: Format, Tl: FormatSet> FormatSet for Cons<H, Tl> {
    const LEN: usize = 1 + Tl::LEN;

    fn media_type(index: usize) -> MediaType {
        match index {
            0 => H::MEDIA_TYPE,
            _ => Tl::media_type(index - 1),
        }
    }
}

impl<T, H: Encode<T>, Tl: EncodeSet<T>> EncodeSet<T> for Cons<H, Tl> {
    fn encode(index: usize, value: &T, parts: &Parts) -> Result<Bytes, FormatError> {
        match index {
            0 => H::encode(value, parts),
            _ => Tl::encode(index - 1, value, parts),
        }
    }
}

impl<T, H: Decode<T>, Tl: DecodeSet<T>> DecodeSet<T> for Cons<H, Tl> {
    fn decode(index: usize, bytes: &[u8]) -> Result<T, FormatError> {
        match index {
            0 => H::decode(bytes),
            _ => Tl::decode(index - 1, bytes),
        }
    }
}

macro_rules! tuple_set {
    ($len:expr; $($format:ident : $index:tt),+) => {
        impl<$($format: Format),+> FormatSet for ($($format,)+) {
            const LEN: usize = $len;

            fn media_type(index: usize) -> MediaType {
                match index {
                    $($index => $format::MEDIA_TYPE,)+
                    _ => MediaType::new(""),
                }
            }
        }

        impl<T, $($format: Encode<T>),+> EncodeSet<T> for ($($format,)+) {
            fn encode(index: usize, value: &T, parts: &Parts) -> Result<Bytes, FormatError> {
                match index {
                    $($index => $format::encode(value, parts),)+
                    _ => Err(FormatError::NotAcceptable),
                }
            }
        }

        impl<T, $($format: Decode<T>),+> DecodeSet<T> for ($($format,)+) {
            fn decode(index: usize, bytes: &[u8]) -> Result<T, FormatError> {
                match index {
                    $($index => $format::decode(bytes),)+
                    _ => Err(FormatError::UnsupportedMediaType),
                }
            }
        }
    };
}

tuple_set!(1; A: 0);
tuple_set!(2; A: 0, B: 1);
tuple_set!(3; A: 0, B: 1, C: 2);
tuple_set!(4; A: 0, B: 1, C: 2, D: 3);
tuple_set!(5; A: 0, B: 1, C: 2, D: 3, E: 4);
tuple_set!(6; A: 0, B: 1, C: 2, D: 3, E: 4, F: 5);

macro_rules! impl_set {
    ($vis:vis $ident:ident = ($feat:literal, $ty:ty); prev = $prev:ty) => {
        #[cfg(feature = $feat)]
        $vis type $ident = Cons<$ty, $prev>;
        #[cfg(not(feature = $feat))]
        $vis type $ident = $prev;
    };
}

type Set0 = Nil;

impl_set!(Set1 = ("xml", crate::formats::Xml); prev = Set0);
impl_set!(Set2 = ("msgpack", crate::formats::MessagePack); prev = Set1);
impl_set!(Set3 = ("cbor", crate::formats::Cbor); prev = Set2);
impl_set!(Set4 = ("toml", crate::formats::Toml); prev = Set3);
impl_set!(Set5 = ("yaml", crate::formats::Yaml); prev = Set4);
impl_set!(Set6 = ("form", crate::formats::Form); prev = Set5);
impl_set!(Set7 = ("json", crate::formats::Json); prev = Set6);

/// Every format whose Cargo feature is enabled, JSON first; the default set of `Negotiated` and `Body`.
pub type Enabled = Set7;
