use crate::Response;

/// What an endpoint made of a request it was offered.
#[derive(Debug)]
pub enum Attempt<B> {
    /// The endpoint handled the request.
    Done(Response),
    /// The endpoint turned the request down before reading its body, which is handed back.
    Declined(B),
    /// The endpoint read the body for a guard and then turned the request down.
    ///
    /// The dispatcher replays the body to the next endpoint when it buffered it beforehand, which
    /// it does for an endpoint whose [`Endpoint::guards_read_body`](crate::Endpoint::guards_read_body)
    /// is `true`; otherwise it answers with the guard miss.
    Missed,
}
