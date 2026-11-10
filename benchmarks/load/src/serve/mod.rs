//! Shared startup for the `serve-*` binaries: options, the control socket and the ready line the
//! harness waits for.

use std::{
    io::{self, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
};

use tokio::runtime::Runtime;

pub mod alloc;
pub mod control;
pub mod pin;

/// First word of the line a server prints once it accepts connections, followed by the app and
/// control socket addresses.
pub const READY: &str = "listening";

/// Address every server binds: loopback with an OS-assigned port.
pub const BIND: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));

/// Options the harness passes to a server.
#[derive(Debug, Clone)]
pub struct Options {
    /// Worker threads; actix runs one single-threaded runtime per worker instead.
    pub workers: usize,
    /// CPUs the worker threads are pinned to; unpinned when `None`.
    pub cpus: Option<Vec<usize>>,
}

impl Options {
    /// Reads the process arguments; see [`Options::parse`].
    pub fn from_args() -> Result<Self, ServeError> {
        Self::parse(std::env::args().skip(1))
    }

    /// Reads `--workers N` (default: one worker per core) and `--cpus LIST`, a comma list of CPUs
    /// and `a-b` ranges.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, ServeError> {
        let mut args = args.into_iter();
        let mut cpus = None;
        let mut workers = std::thread::available_parallelism().map_or(1, usize::from);

        while let Some(arg) = args.next() {
            match (arg.as_str(), args.next()) {
                ("--workers", Some(value)) => {
                    workers = value
                        .parse()
                        .map_err(|_| ServeError::Args(format!("invalid worker count `{value}`")))?;
                }
                ("--cpus", Some(value)) => cpus = Some(cpu_list(&value)?),
                _ => return Err(ServeError::Args(format!("unexpected argument `{arg}`"))),
            }
        }

        Ok(Self { workers, cpus })
    }
}

/// Startup failure of a `serve-*` binary.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    /// The command line could not be parsed.
    #[error("{0}; usage: serve-<framework> [--workers N] [--cpus LIST]")]
    Args(String),
    /// A CPU id does not fit a CPU set.
    #[error("CPU {0} is out of range, the highest supported id is {max}", max = pin::MAX_CPUS - 1)]
    CpuOutOfRange(usize),
    /// The CPU list cannot be pinned to, such as a CPU the machine does not have.
    #[error("cannot pin to CPUs {cpus:?}: {source}")]
    Pin {
        /// The requested CPUs.
        cpus: Vec<usize>,
        /// The OS error.
        source: io::Error,
    },
    /// Binding, the runtime or the server itself failed.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Tells the harness where the app and the control socket listen.
pub fn announce(app: SocketAddr, control: SocketAddr) -> io::Result<()> {
    let mut out = io::stdout().lock();

    writeln!(out, "{READY} {app} {control}")?;

    out.flush()
}

fn cpu_list(value: &str) -> Result<Vec<usize>, ServeError> {
    let invalid = || ServeError::Args(format!("invalid CPU list `{value}`"));
    let mut cpus = Vec::new();

    for item in value.split(',') {
        let (first, last) = item.split_once('-').unwrap_or((item, item));
        let first: usize = first.parse().map_err(|_| invalid())?;
        let last: usize = last.parse().map_err(|_| invalid())?;

        if first > last {
            return Err(invalid());
        }

        if let Some(cpu) = [first, last].into_iter().find(|cpu| *cpu >= pin::MAX_CPUS) {
            return Err(ServeError::CpuOutOfRange(cpu));
        }

        cpus.extend(first..=last);
    }

    Ok(cpus)
}

/// Multi-thread tokio runtime with `workers` worker threads, each pinned to `cpus` when given.
/// Fails when `cpus` cannot be pinned to; a worker that still fails to pin panics.
pub fn runtime(workers: usize, cpus: Option<Vec<usize>>) -> Result<Runtime, ServeError> {
    let mut builder = tokio::runtime::Builder::new_multi_thread();

    builder.worker_threads(workers).enable_all();

    if let Some(cpus) = cpus {
        check_pinning(&cpus)?;
        builder.on_thread_start(move || pin::require(&cpus));
    }

    Ok(builder.build()?)
}

/// Fails with [`ServeError::Pin`] when the calling process cannot pin a thread to `cpus`.
pub fn check_pinning(cpus: &[usize]) -> Result<(), ServeError> {
    pin::check(cpus).map_err(|source| ServeError::Pin {
        cpus: cpus.to_vec(),
        source,
    })
}

#[cfg(test)]
mod tests;
