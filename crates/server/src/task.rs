//! Task hooks, defined in `fast_http_core` so handler-side crates can use them without the server.

pub use fast_http_core::{Plain, Task, TaskSpawner, TaskWrap, WithSpawner, with_spawner};

#[cfg(test)]
mod tests;
