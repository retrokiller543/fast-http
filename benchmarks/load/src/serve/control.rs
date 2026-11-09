//! Control socket on its own thread, outside the measured server: one line-based command per
//! connection, `start` or `stop`.

use std::{
    io::{self, BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    thread,
};

use super::{BIND, alloc};

/// Starts counting allocations; answered with `ok`.
pub const START: &str = "start";

/// Stops counting; answered with `<allocs> <bytes>`.
pub const STOP: &str = "stop";

/// Binds the control socket and serves it on a background thread.
pub fn spawn() -> io::Result<SocketAddr> {
    let listener = TcpListener::bind(BIND)?;
    let addr = listener.local_addr()?;

    thread::Builder::new()
        .name("load-control".to_owned())
        .spawn(move || serve(&listener))?;

    Ok(addr)
}

fn serve(listener: &TcpListener) {
    for stream in listener.incoming().flatten() {
        // A failed exchange only fails that one harness command, which reports it.
        let _ = exchange(stream);
    }
}

fn exchange(stream: TcpStream) -> io::Result<()> {
    let mut writer = stream.try_clone()?;
    let mut line = String::new();

    BufReader::new(stream).read_line(&mut line)?;

    match line.trim() {
        START => {
            alloc::start();

            writer.write_all(b"ok\n")
        }
        STOP => {
            let counts = alloc::stop();

            writeln!(writer, "{} {}", counts.allocs, counts.bytes)
        }
        other => writeln!(writer, "error: unknown command `{other}`"),
    }
}
