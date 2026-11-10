//! Streaming `multipart/form-data` extraction for fast-http, shaped after
//! `axum::extract::Multipart`.
//!
//! [`Multipart`] is a body extractor, so it is the last handler argument. Fields are read one at
//! a time with [`Multipart::next_field`] and each can be streamed in chunks, so an upload never
//! has to fit in memory.

pub mod error;
pub mod extract;
pub mod field;

pub use error::MultipartError;
pub use extract::{DEFAULT_FIELD_LIMIT, DEFAULT_WHOLE_LIMIT, Multipart};
pub use field::Field;
