//! Network load comparison of fast-http, axum and actix-web.
//!
//! Each framework runs as its own `serve-*` binary, so its memory is measured alone. The `load`
//! binary starts them one at a time, drives them with `oha`, samples their RSS and switches their
//! allocation counter on and off through a control socket.

pub mod harness;
pub mod serve;
