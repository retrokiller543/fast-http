use std::convert::Infallible;

use http::{Method, Uri, Version, request::Parts};

use crate::{ErrorResponse, ReqBody};

mod extension;
mod path;

pub use extension::{Extension, PeerAddr};
pub use path::{FromParams, FromPathParam, Path, PathArgs, PathParams, Segments};

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
