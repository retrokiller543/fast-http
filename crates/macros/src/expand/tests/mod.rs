use quote::quote;

use super::*;
use crate::route::HttpMethod;

fn expand(args: TokenStream, item: TokenStream) -> String {
    controller(args, item)
        .expect("expands")
        .to_string()
        .replace(' ', "")
}

fn error(args: TokenStream, item: TokenStream) -> String {
    controller(args, item)
        .map(drop)
        .expect_err("error")
        .to_string()
}

fn position(haystack: &str, needle: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is missing from {haystack}"))
}

fn expand_routes(args: TokenStream, item: TokenStream) -> String {
    routes(args, item)
        .expect("expands")
        .to_string()
        .replace(' ', "")
}

fn expand_standalone(method: HttpMethod, args: TokenStream, item: TokenStream) -> String {
    standalone(method, args, item)
        .expect("expands")
        .to_string()
        .replace(' ', "")
}

mod controller;
mod errors;
mod free;
mod guards;
mod recover;
