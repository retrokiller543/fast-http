use std::{
    collections::{BTreeMap, HashMap},
    hash::BuildHasher,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    num::{
        NonZeroI8, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI128, NonZeroIsize, NonZeroU8,
        NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU128, NonZeroUsize,
    },
};

use http::request::Parts;

use crate::{ErrorResponse, FromParts, Rejection};

/// Path parameters of a matched route: static names from the pattern, values borrowed from the request path.
#[derive(Debug, Clone, Copy)]
pub struct PathParams<'a> {
    names: &'a [&'static str],
    values: &'a [&'a str],
}

impl<'a> PathParams<'a> {
    /// No parameters.
    pub const EMPTY: PathParams<'static> = PathParams {
        names: &[],
        values: &[],
    };

    /// Pairs names with values; extra names or values are ignored.
    pub const fn new(names: &'a [&'static str], values: &'a [&'a str]) -> Self {
        Self { names, values }
    }

    /// Number of parameters.
    pub const fn len(&self) -> usize {
        if self.names.len() < self.values.len() {
            self.names.len()
        } else {
            self.values.len()
        }
    }

    /// Whether there are no parameters.
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Raw value of the parameter called `name`.
    pub fn get(&self, name: &str) -> Option<&'a str> {
        let index = self.names.iter().position(|candidate| *candidate == name)?;

        self.values.get(index).copied()
    }

    /// Splits a `**` capture into its segments without allocating.
    pub fn segments(&self, name: &str) -> Option<Segments<'a>> {
        self.get(name).map(Segments)
    }

    /// Name and raw value pairs in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &'a str)> + '_ {
        self.names.iter().copied().zip(self.values.iter().copied())
    }

    /// Parses the parameter at `index`, borrowing from the request path where `T` allows it.
    pub fn parse_at<T: FromPathParam>(&self, index: usize) -> Result<T::At<'a>, Rejection> {
        let name = self.names.get(index).copied().unwrap_or("?");
        let raw = self
            .values
            .get(index)
            .copied()
            .ok_or_else(|| Rejection::BadPathParam {
                name,
                message: "parameter missing".to_owned(),
            })?;

        parse_named::<T>(name, raw)
    }

    /// Parses the parameter called `name`, borrowing from the request path where `T` allows it.
    pub fn parse<T: FromPathParam>(&self, name: &'static str) -> Result<T::At<'a>, Rejection> {
        let raw = self.get(name).ok_or_else(|| Rejection::BadPathParam {
            name,
            message: "parameter missing".to_owned(),
        })?;

        parse_named::<T>(name, raw)
    }
}

/// Parses one raw path segment into a typed value.
///
/// `At<'r>` is the value produced for a path borrowed for `'r`: `Self` for owned types,
/// `&'r str` for `&str`, so handlers can borrow straight from the request.
pub trait FromPathParam {
    /// Value produced when the raw segment lives for `'r`.
    type At<'r>: Send;

    /// Parses the raw segment, describing failures as text.
    fn from_path_param<'r>(raw: &'r str) -> Result<Self::At<'r>, String>;
}

impl FromPathParam for &str {
    type At<'r> = &'r str;

    fn from_path_param<'r>(raw: &'r str) -> Result<Self::At<'r>, String> {
        Ok(raw)
    }
}

macro_rules! from_path_param_parse {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl FromPathParam for $ty {
                type At<'r> = Self;

                fn from_path_param(raw: &str) -> Result<Self, String> {
                    raw.parse().map_err(|error: <$ty as std::str::FromStr>::Err| error.to_string())
                }
            }
        )+
    };
}

from_path_param_parse!(
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64,
    bool,
    char,
    String,
    NonZeroU8,
    NonZeroU16,
    NonZeroU32,
    NonZeroU64,
    NonZeroU128,
    NonZeroUsize,
    NonZeroI8,
    NonZeroI16,
    NonZeroI32,
    NonZeroI64,
    NonZeroI128,
    NonZeroIsize,
    IpAddr,
    Ipv4Addr,
    Ipv6Addr,
);

/// Remaining path segments captured by `**`, borrowed from the request path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segments<'a>(&'a str);

impl<'a> Segments<'a> {
    /// Wraps a raw `a/b/c` capture.
    pub const fn new(raw: &'a str) -> Self {
        Self(raw)
    }

    /// The raw capture, slashes included.
    pub const fn as_str(&self) -> &'a str {
        self.0
    }

    /// Iterates the segments in order.
    pub fn iter(&self) -> std::str::Split<'a, char> {
        self.0.split('/')
    }

    /// Number of segments.
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// Whether the capture is empty.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Segment at `index`.
    pub fn get(&self, index: usize) -> Option<&'a str> {
        self.iter().nth(index)
    }
}

impl<'a> IntoIterator for Segments<'a> {
    type Item = &'a str;
    type IntoIter = std::str::Split<'a, char>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl FromPathParam for Segments<'_> {
    type At<'r> = Segments<'r>;

    fn from_path_param<'r>(raw: &'r str) -> Result<Self::At<'r>, String> {
        Ok(Segments(raw))
    }
}

/// Extracts a value from the request head and the matched path parameters.
///
/// Every [`FromParts`] extractor qualifies with `At<'r> = Self`. Borrowing extractors such as
/// `&Parts` or `Path<&str>` produce `At<'r>` values tied to the request instead.
pub trait FromParams {
    /// Error rendered as the response when extraction fails.
    type Rejection: ErrorResponse;

    /// Value produced for a request borrowed for `'r`.
    type At<'r>: Send;

    /// Extracts from the head and the parameters.
    fn from_params<'r>(
        parts: &'r Parts,
        params: &PathParams<'r>,
    ) -> Result<Self::At<'r>, Self::Rejection>;
}

/// Extracts a value of the type the handler wrote, from a request borrowed for `'r`.
///
/// Where [`FromParams`] names the produced value through an associated type, which cannot say
/// "this struct, with its lifetime replaced by `'r`", this trait is implemented for the value type
/// itself, so a generated endpoint can ask for `Query<Params<'_>>` and let inference pick the
/// request lifetime. Every [`FromParams`] extractor whose value is its own type qualifies.
pub trait FromParamsAt<'r>: Sized + Send {
    /// Error rendered as the response when extraction fails.
    type Rejection: ErrorResponse;

    /// Extracts from the head and the parameters.
    fn from_params_at(parts: &'r Parts, params: &PathParams<'r>) -> Result<Self, Self::Rejection>;
}

impl<'r, T: FromParams<At<'r> = T> + Send> FromParamsAt<'r> for T {
    type Rejection = T::Rejection;

    fn from_params_at(parts: &'r Parts, params: &PathParams<'r>) -> Result<Self, Self::Rejection> {
        T::from_params(parts, params)
    }
}

impl<T: FromParts + Send> FromParams for T {
    type Rejection = T::Rejection;
    type At<'r> = T;

    fn from_params<'r>(parts: &'r Parts, _: &PathParams<'r>) -> Result<T, Self::Rejection> {
        T::from_parts(parts)
    }
}

/// Shapes a [`Path`] can take: tuples by position, scalars for a single parameter, maps by name.
pub trait PathArgs {
    /// Value produced when the path is borrowed for `'r`.
    type At<'r>: Send;

    /// Builds the value from the matched parameters.
    fn from_path_params<'r>(params: &PathParams<'r>) -> Result<Self::At<'r>, Rejection>;
}

/// Typed access to the path parameters of the matched route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Path<T>(pub T);

impl<T: PathArgs> FromParams for Path<T> {
    type Rejection = Rejection;
    type At<'r> = Path<T::At<'r>>;

    fn from_params<'r>(
        _: &'r Parts,
        params: &PathParams<'r>,
    ) -> Result<Self::At<'r>, Self::Rejection> {
        T::from_path_params(params).map(Path)
    }
}

macro_rules! path_args_scalar {
    ($([$($lt:lifetime)?] $ty:ty),+ $(,)?) => {
        $(
            impl<$($lt)?> PathArgs for $ty {
                type At<'r> = <$ty as FromPathParam>::At<'r>;

                fn from_path_params<'r>(params: &PathParams<'r>) -> Result<Self::At<'r>, Rejection> {
                    params.parse_at::<$ty>(0)
                }
            }
        )+
    };
}

path_args_scalar!(
    [] u8, [] u16, [] u32, [] u64, [] u128, [] usize, [] i8, [] i16, [] i32, [] i64, [] i128,
    [] isize, [] f32, [] f64, [] bool, [] char, [] String, [] NonZeroU8, [] NonZeroU16,
    [] NonZeroU32, [] NonZeroU64, [] NonZeroU128, [] NonZeroUsize, [] NonZeroI8, [] NonZeroI16,
    [] NonZeroI32, [] NonZeroI64, [] NonZeroI128, [] NonZeroIsize, [] IpAddr, [] Ipv4Addr,
    [] Ipv6Addr, ['a] &'a str, ['a] Segments<'a>,
);

macro_rules! path_args_tuple {
    ($($ty:ident : $index:tt),+) => {
        impl<$($ty: FromPathParam),+> PathArgs for ($($ty,)+) {
            type At<'r> = ($($ty::At<'r>,)+);

            fn from_path_params<'r>(params: &PathParams<'r>) -> Result<Self::At<'r>, Rejection> {
                Ok(($(params.parse_at::<$ty>($index)?,)+))
            }
        }
    };
}

path_args_tuple!(A: 0);
path_args_tuple!(A: 0, B: 1);
path_args_tuple!(A: 0, B: 1, C: 2);
path_args_tuple!(A: 0, B: 1, C: 2, D: 3);
path_args_tuple!(A: 0, B: 1, C: 2, D: 3, E: 4);
path_args_tuple!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5);

impl<V: FromPathParam, S: BuildHasher + Default + Send> PathArgs for HashMap<String, V, S> {
    type At<'r> = HashMap<String, V::At<'r>, S>;

    fn from_path_params<'r>(params: &PathParams<'r>) -> Result<Self::At<'r>, Rejection> {
        params
            .iter()
            .map(|(name, raw)| Ok((name.to_owned(), parse_named::<V>(name, raw)?)))
            .collect()
    }
}

impl<V: FromPathParam, S: BuildHasher + Default + Send> PathArgs for HashMap<&str, V, S> {
    type At<'r> = HashMap<&'r str, V::At<'r>, S>;

    fn from_path_params<'r>(params: &PathParams<'r>) -> Result<Self::At<'r>, Rejection> {
        params
            .iter()
            .map(|(name, raw)| Ok((name, parse_named::<V>(name, raw)?)))
            .collect()
    }
}

impl<V: FromPathParam> PathArgs for BTreeMap<String, V> {
    type At<'r> = BTreeMap<String, V::At<'r>>;

    fn from_path_params<'r>(params: &PathParams<'r>) -> Result<Self::At<'r>, Rejection> {
        params
            .iter()
            .map(|(name, raw)| Ok((name.to_owned(), parse_named::<V>(name, raw)?)))
            .collect()
    }
}

impl<V: FromPathParam> PathArgs for BTreeMap<&str, V> {
    type At<'r> = BTreeMap<&'r str, V::At<'r>>;

    fn from_path_params<'r>(params: &PathParams<'r>) -> Result<Self::At<'r>, Rejection> {
        params
            .iter()
            .map(|(name, raw)| Ok((name, parse_named::<V>(name, raw)?)))
            .collect()
    }
}

fn parse_named<'r, V: FromPathParam>(
    name: &'static str,
    raw: &'r str,
) -> Result<V::At<'r>, Rejection> {
    V::from_path_param(raw).map_err(|message| Rejection::BadPathParam { name, message })
}
