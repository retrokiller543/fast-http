//! Data formats for requests and responses: feature-gated serde formats, content negotiation,
//! streaming responses and opt-in RFC 9457 problem details.

mod body;
mod error;
mod format;
mod negotiated;
#[cfg(feature = "form")]
mod query;
mod set;

pub mod formats;
pub mod stream;

#[cfg(feature = "problem")]
pub mod problem;

pub use body::{Body, DEFAULT_BODY_LIMIT};
pub use error::FormatError;
pub use format::{Decode, Encode, Format};
pub use negotiated::Negotiated;
#[cfg(feature = "form")]
pub use query::{Query, RawQuery};
pub use set::{Cons, DecodeSet, Enabled, EncodeSet, FormatSet, Nil};
