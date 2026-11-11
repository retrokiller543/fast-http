use crate::Response;

/// What an endpoint made of a request it was offered.
#[derive(Debug)]
pub enum Attempt<B> {
    /// The endpoint handled the request.
    Done(Response),
    /// The endpoint turned the request down before reading its body, which is handed back.
    Declined(B),
}
