//! Data formats for requests and responses: feature-gated serde formats, content negotiation,
//! streaming responses and opt-in RFC 9457 problem details.

mod body;
mod error;
mod format;
mod negotiated;
#[cfg(feature = "form")]
mod query;
mod set;
mod wrapper;

#[cfg(any(
    feature = "json",
    feature = "form",
    feature = "yaml",
    feature = "toml",
    feature = "cbor",
    feature = "msgpack",
    feature = "xml"
))]
pub mod formats;
pub mod stream;

#[cfg(feature = "problem")]
pub mod problem;

pub use body::{Body, DEFAULT_BODY_LIMIT};
pub use error::FormatError;
pub use format::{Decode, Encode, Format};
#[cfg(any(
    feature = "json",
    feature = "form",
    feature = "yaml",
    feature = "toml",
    feature = "cbor",
    feature = "msgpack",
    feature = "xml"
))]
pub use formats::*;
pub use negotiated::Negotiated;
#[cfg(feature = "form")]
pub use query::{BorrowedQueryField, Query};
pub use set::{Cons, DecodeSet, Enabled, EncodeSet, FormatSet, Nil};
