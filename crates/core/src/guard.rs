use bytes::Bytes;
use http::request::Parts;

/// When an endpoint's guards can be evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GuardPhase {
    /// Sync and allocation-free, runs on the request head only.
    Head,
    /// Needs the buffered body, which the server reads once up to its limit.
    Body,
}

/// Selects an endpoint variant from the request head (query, header, `Accept`, version, ...).
pub trait HeadGuard {
    /// Typed value forced onto the handler when the guard matches.
    type Output;

    /// Returns the value when the guard matches.
    fn check(parts: &Parts) -> Option<Self::Output>;
}

/// Selects an endpoint variant from the buffered body; the server re-materializes the body for the handler.
pub trait BodyGuard {
    /// Typed value forced onto the handler when the guard matches.
    type Output;

    /// Returns the value when the guard matches.
    fn check(parts: &Parts, body: &Bytes) -> Option<Self::Output>;
}
