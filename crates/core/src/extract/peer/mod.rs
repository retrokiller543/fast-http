//! The connected peer's address, carried around each poll of a request instead of in the
//! request's extensions, which would allocate per request.

use std::{
    any::type_name,
    cell::Cell,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use http::request::Parts;
use pin_project_lite::pin_project;

use crate::{FromParts, Rejection};

#[cfg(test)]
mod tests;

thread_local! {
    static PEER: Cell<Option<SocketAddr>> = const { Cell::new(None) };
}

/// Address of the connected peer, set by the engine with [`with_peer`] or inserted as an extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerAddr(pub SocketAddr);

impl FromParts for PeerAddr {
    type Rejection = Rejection;

    fn from_parts(parts: &Parts) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<PeerAddr>()
            .copied()
            .or_else(|| PEER.get().map(PeerAddr))
            .ok_or(Rejection::MissingExtension {
                name: type_name::<PeerAddr>(),
            })
    }
}

pin_project! {
    /// A request future that sees `peer` as its [`PeerAddr`] while it is polled.
    pub struct WithPeer<F> {
        peer: Option<SocketAddr>,
        #[pin]
        future: F,
    }
}

/// Runs `future` with `peer` as the [`PeerAddr`] its extractors read; `None` hides any outer peer.
pub fn with_peer<F: Future>(peer: Option<SocketAddr>, future: F) -> WithPeer<F> {
    WithPeer { peer, future }
}

impl<F: Future> Future for WithPeer<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = self.project();
        let _restore = Restore(PEER.replace(*this.peer));

        this.future.poll(cx)
    }
}

/// Puts the outer peer back when a poll ends, including by panic.
struct Restore(Option<SocketAddr>);

impl Drop for Restore {
    fn drop(&mut self) {
        PEER.set(self.0);
    }
}
