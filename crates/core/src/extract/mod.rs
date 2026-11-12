use std::convert::Infallible;

use http::{Extensions, HeaderMap, Method, Uri, Version, request::Parts};

use crate::{ErrorResponse, ReqBody};

mod extension;
mod path;
mod peer;
mod query;
mod transparent;

pub use extension::Extension;
pub use path::{FromParams, FromParamsAt, FromPathParam, Path, PathArgs, PathParams, Segments};
pub use peer::{PeerAddr, WithPeer, with_peer};
pub use query::QueryParams;
pub use transparent::Transparent;

/// Extracts a value from the request head; any number may precede a [`FromRequest`].
pub trait FromParts: Sized {
    /// Error rendered as the response when extraction fails.
    type Rejection: ErrorResponse;

    /// Extracts from the request head.
    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection>;
}

/// Extracts a value from the head and consumes the body; at most one, and last.
pub trait FromRequest: Sized {
    /// Error rendered as the response when extraction fails.
    type Rejection: ErrorResponse;

    /// Extracts from the head and the body.
    fn from_request<B: ReqBody>(
        parts: &Parts,
        body: B,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send;
}

impl FromParts for Method {
    type Rejection = Infallible;

    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection> {
        Ok(parts.method.clone())
    }
}

impl FromParts for Uri {
    type Rejection = Infallible;

    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection> {
        Ok(parts.uri.clone())
    }
}

impl FromParts for Version {
    type Rejection = Infallible;

    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection> {
        Ok(parts.version)
    }
}

impl<T: FromParts> FromParts for Option<T> {
    type Rejection = Infallible;

    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection> {
        Ok(T::from_parts(parts).ok())
    }
}

impl<T: FromParts> FromParts for Result<T, T::Rejection> {
    type Rejection = Infallible;

    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection> {
        Ok(T::from_parts(parts))
    }
}

macro_rules! borrowed_head {
    ($($ty:ty => |$parts:ident| $field:expr),+ $(,)?) => {
        $(
            impl<'a> FromParams for &'a $ty {
                type Rejection = Infallible;
                type At<'r> = &'r $ty;

                fn from_params<'r>(
                    $parts: &'r Parts,
                    _: &PathParams<'r>,
                ) -> Result<Self::At<'r>, Self::Rejection> {
                    Ok($field)
                }
            }
        )+
    };
}

borrowed_head!(
    Parts => |parts| parts,
    HeaderMap => |parts| &parts.headers,
    Method => |parts| &parts.method,
    Uri => |parts| &parts.uri,
    Extensions => |parts| &parts.extensions,
);
