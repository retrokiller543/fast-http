use std::{any::type_name, convert::Infallible, net::SocketAddr};

use http::{HeaderMap, request::Parts};

use crate::{FromParts, Rejection};

/// Clones a value an earlier layer or the engine stored in the request extensions.
#[derive(Debug, Clone)]
pub struct Extension<T>(pub T);

impl<T: Clone + Send + Sync + 'static> FromParts for Extension<T> {
    type Rejection = Rejection;

    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<T>()
            .cloned()
            .map(Extension)
            .ok_or(Rejection::MissingExtension {
                name: type_name::<T>(),
            })
    }
}

/// Address of the connected peer, inserted into the extensions by the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerAddr(pub SocketAddr);

impl FromParts for PeerAddr {
    type Rejection = Rejection;

    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<PeerAddr>()
            .copied()
            .ok_or(Rejection::MissingExtension {
                name: type_name::<PeerAddr>(),
            })
    }
}

impl FromParts for HeaderMap {
    type Rejection = Infallible;

    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection> {
        Ok(parts.headers.clone())
    }
}
