//! Shared startup for the `serve-*` binaries: options, the control socket and the ready line the
//! harness waits for.

use std::{
    io::{self, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
};

use tokio::runtime::Runtime;

pub mod alloc;
pub mod control;

/// First word of the line a server prints once it accepts connections, followed by the app and
/// control socket addresses.
pub const READY: &str = "listening";

/// Address every server binds: loopback with an OS-assigned port.
pub const BIND: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));

/// Options the harness passes to a server.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Worker threads; actix runs one single-threaded runtime per worker instead.
    pub workers: usize,
}

impl Options {
    /// Reads `--workers N`; defaults to one worker per core.
    pub fn from_args() -> Result<Self, ServeError> {
        let mut args = std::env::args().skip(1);
        let mut workers = std::thread::available_parallelism().map_or(1, usize::from);

        while let Some(arg) = args.next() {
            match (arg.as_str(), args.next()) {
                ("--workers", Some(value)) => {
                    workers = value
                        .parse()
                        .map_err(|_| ServeError::Args(format!("invalid worker count `{value}`")))?;
                }
                _ => return Err(ServeError::Args(format!("unexpected argument `{arg}`"))),
            }
        }

        Ok(Self { workers })
    }
}

/// Startup failure of a `serve-*` binary.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    /// The command line could not be parsed.
    #[error("{0}; usage: serve-<framework> [--workers N]")]
    Args(String),
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

/// Multi-thread tokio runtime with `workers` worker threads.
pub fn runtime(workers: usize) -> io::Result<Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
}
