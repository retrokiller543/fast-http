//! Hook around every task the server spawns, so an embedding framework can carry its own
//! context, such as a DI invocation context or a tracing span, into connection and stream tasks,
//! and a handle that lets handlers spawn detached tasks through the same hook.

use std::{
    cell::Cell,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use http::request::Parts;
use pin_project_lite::pin_project;

use crate::{FromParts, Rejection};

#[cfg(test)]
mod tests;

thread_local! {
    static SPAWNER: Cell<Option<TaskSpawner>> = const { Cell::new(None) };
}

/// Wraps each task before the server spawns it: connection tasks, the per-stream tasks the
/// protocol engine spawns for HTTP/2 and the detached tasks handlers spawn through a
/// [`TaskSpawner`].
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

/// A type-erased task, as handed to the closure of a [`TaskSpawner`].
pub type Task = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Spawns detached tasks the way the server spawns its own, through its [`TaskWrap`].
///
/// A handler gets one with [`TaskSpawner::current`] or as an extractor argument. Use it for work
/// that outlives the request, such as an upgraded WebSocket, so the task runs in the same
/// context as the request that started it. Each spawn boxes the task once.
#[derive(Clone)]
pub struct TaskSpawner(Arc<dyn Fn(Task) + Send + Sync>);

impl std::fmt::Debug for TaskSpawner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TaskSpawner")
    }
}

impl TaskSpawner {
    /// Builds a spawner that hands every task to `spawn`; the engine passes its runtime's spawn
    /// function composed with the server's [`TaskWrap`].
    pub fn new(spawn: impl Fn(Task) + Send + Sync + 'static) -> Self {
        Self(Arc::new(spawn))
    }

    /// Spawns `task` detached from the current request.
    pub fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) {
        (self.0)(Box::pin(task));
    }

    /// The spawner of the request being polled, set by the engine with [`with_spawner`].
    pub fn current() -> Option<Self> {
        SPAWNER.with(|slot| {
            let held = slot.take();
            let copy = held.clone();

            slot.set(held);

            copy
        })
    }
}

impl FromParts for TaskSpawner {
    type Rejection = Rejection;

    fn from_parts(_: &Parts) -> Result<Self, Self::Rejection> {
        Self::current().ok_or(Rejection::MissingExtension {
            name: std::any::type_name::<Self>(),
        })
    }
}

pin_project! {
    /// A request future that sees `spawner` as its [`TaskSpawner::current`] while it is polled.
    pub struct WithSpawner<F> {
        spawner: Option<TaskSpawner>,
        #[pin]
        future: F,
    }
}

/// Runs `future` with `spawner` as the [`TaskSpawner`] its handlers can reach; `None` hides any
/// outer spawner.
pub fn with_spawner<F: Future>(spawner: Option<TaskSpawner>, future: F) -> WithSpawner<F> {
    WithSpawner { spawner, future }
}

impl<F: Future> Future for WithSpawner<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = self.project();
        let _restore = Restore {
            outer: SPAWNER.replace(this.spawner.take()),
            slot: this.spawner,
        };

        this.future.poll(cx)
    }
}

/// Moves the spawner back out of the thread-local and puts the outer one back when a poll ends,
/// including by panic.
struct Restore<'a> {
    outer: Option<TaskSpawner>,
    slot: &'a mut Option<TaskSpawner>,
}

impl Drop for Restore<'_> {
    fn drop(&mut self) {
        *self.slot = SPAWNER.replace(self.outer.take());
    }
}
