use std::{
    collections::{BTreeMap, HashMap},
    fmt::Display,
    hash::BuildHasher,
    str::FromStr,
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
    pub fn segments(&self, name: &str) -> Option<impl Iterator<Item = &'a str>> {
        self.get(name).map(|raw| raw.split('/'))
    }

    /// Name and raw value pairs in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &'a str)> + '_ {
        self.names.iter().copied().zip(self.values.iter().copied())
    }

    /// Parses the parameter at `index`.
    pub fn parse_at<T: FromPathParam>(&self, index: usize) -> Result<T, Rejection> {
        let name = self.names.get(index).copied().unwrap_or("?");
        let raw = self
            .values
            .get(index)
            .copied()
            .ok_or(Rejection::BadPathParam {
                name,
                message: "parameter missing".to_owned(),
            })?;

        T::from_path_param(raw).map_err(|message| Rejection::BadPathParam { name, message })
    }

    /// Parses the parameter called `name`.
    pub fn parse<T: FromPathParam>(&self, name: &'static str) -> Result<T, Rejection> {
        let raw = self.get(name).ok_or(Rejection::BadPathParam {
            name,
            message: "parameter missing".to_owned(),
        })?;

        T::from_path_param(raw).map_err(|message| Rejection::BadPathParam { name, message })
    }
}

/// Parses one raw path segment into a typed value.
pub trait FromPathParam: Sized {
    /// Parses the raw segment, describing failures as text.
    fn from_path_param(raw: &str) -> Result<Self, String>;
}

impl<T> FromPathParam for T
where
    T: FromStr,
    T::Err: Display,
{
    fn from_path_param(raw: &str) -> Result<Self, String> {
        raw.parse().map_err(|error: T::Err| error.to_string())
    }
}

/// Remaining path segments captured by `**`, split on `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segments(pub Vec<String>);

impl FromPathParam for Segments {
    fn from_path_param(raw: &str) -> Result<Self, String> {
        Ok(Self(raw.split('/').map(str::to_owned).collect()))
    }
}

/// Extracts a value from the request head and the matched path parameters.
///
/// Every [`FromParts`] extractor qualifies; [`Path`] is the one that reads the parameters.
pub trait FromParams: Sized {
    /// Error rendered as the response when extraction fails.
    type Rejection: ErrorResponse;

    /// Extracts from the head and the parameters.
    fn from_params(parts: &Parts, params: &PathParams<'_>) -> Result<Self, Self::Rejection>;
}

impl<T: FromParts> FromParams for T {
    type Rejection = T::Rejection;

    fn from_params(parts: &Parts, _: &PathParams<'_>) -> Result<Self, Self::Rejection> {
        T::from_parts(parts)
    }
}

/// Shapes a [`Path`] can take: tuples by position, scalars for a single parameter, maps by name.
pub trait PathArgs: Sized {
    /// Builds the value from the matched parameters.
    fn from_path_params(params: &PathParams<'_>) -> Result<Self, Rejection>;
}

/// Typed access to the path parameters of the matched route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path<T>(pub T);

impl<T: PathArgs> FromParams for Path<T> {
    type Rejection = Rejection;

    fn from_params(_: &Parts, params: &PathParams<'_>) -> Result<Self, Self::Rejection> {
        T::from_path_params(params).map(Path)
    }
}

macro_rules! path_args_scalar {
    ($($ty:ty),+) => {
        $(
            impl PathArgs for $ty {
                fn from_path_params(params: &PathParams<'_>) -> Result<Self, Rejection> {
                    params.parse_at(0)
                }
            }
        )+
    };
}

path_args_scalar!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64, bool, char, String,
    Segments
);

macro_rules! path_args_tuple {
    ($($ty:ident : $index:tt),+) => {
        impl<$($ty: FromPathParam),+> PathArgs for ($($ty,)+) {
            fn from_path_params(params: &PathParams<'_>) -> Result<Self, Rejection> {
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

impl<V: FromPathParam, S: BuildHasher + Default> PathArgs for HashMap<String, V, S> {
    fn from_path_params(params: &PathParams<'_>) -> Result<Self, Rejection> {
        params
            .iter()
            .map(|(name, _)| Ok((name.to_owned(), params.parse(name)?)))
            .collect()
    }
}

impl<V: FromPathParam> PathArgs for BTreeMap<String, V> {
    fn from_path_params(params: &PathParams<'_>) -> Result<Self, Rejection> {
        params
            .iter()
            .map(|(name, _)| Ok((name.to_owned(), params.parse(name)?)))
            .collect()
    }
}
