//! Hook around every task the server spawns, so an embedding framework can carry its own
//! context, such as a DI invocation context or a tracing span, into connection and stream tasks.

/// Wraps each task before the server spawns it: connection tasks and, for HTTP/2, the per-stream
/// tasks the protocol engine spawns.
///
/// The wrapper runs where the spawn happens, so it can capture whatever is current there and
/// re-enter it inside the task. Cancellation, draining and aborting on shutdown are unaffected.
pub trait TaskWrap: Clone + Send + Sync + 'static {
    /// Returns `task`, possibly wrapped. The output is passed through unchanged.
    fn wrap<F>(&self, task: F) -> impl Future<Output = F::Output> + Send + 'static
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static;
}

/// Spawns tasks unchanged.
#[derive(Debug, Clone, Copy, Default)]
pub struct Plain;

impl TaskWrap for Plain {
    fn wrap<F>(&self, task: F) -> impl Future<Output = F::Output> + Send + 'static
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        task
    }
}

#[cfg(test)]
mod tests;
