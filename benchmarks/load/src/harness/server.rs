//! A running `serve-*` process and its control socket.

use std::{
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream},
    process::{Child, ChildStdout, Command, Stdio},
};

use crate::serve::{
    READY,
    alloc::Counts,
    control::{START, STOP},
};

use super::{
    LoadError,
    config::{Framework, Named},
};

/// A server started from its binary next to `load`; killed on drop.
pub struct ServerProcess {
    child: Child,
    app: SocketAddr,
    control: SocketAddr,
    _stdout: BufReader<ChildStdout>,
}

impl ServerProcess {
    /// Starts `framework`'s server and waits for its ready line.
    pub fn spawn(framework: Framework, workers: usize) -> Result<Self, LoadError> {
        let binary = std::env::current_exe()?.with_file_name(framework.binary());
        let mut child = Command::new(&binary)
            .arg("--workers")
            .arg(workers.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|source| LoadError::Spawn {
                binary: binary.clone(),
                source,
            })?;
        let mut stdout = BufReader::new(child.stdout.take().expect("stdout is piped"));
        let mut line = String::new();

        stdout.read_line(&mut line)?;

        let Some((app, control)) = ready(&line) else {
            // The server already failed; its exit status adds nothing to `NotReady`.
            let _ = child.kill();
            let _ = child.wait();

            return Err(LoadError::NotReady(framework.name()));
        };

        Ok(Self {
            child,
            app,
            control,
            _stdout: stdout,
        })
    }

    /// Base URL of the app.
    pub fn base(&self) -> String {
        format!("http://{}", self.app)
    }

    /// Process id, for RSS sampling.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Resets the allocation counters and starts counting.
    pub fn start_counting(&self) -> Result<(), LoadError> {
        let reply = command(self.control, START)?;

        if reply != "ok" {
            return Err(LoadError::Control(reply));
        }

        Ok(())
    }

    /// Stops counting and returns the counts.
    pub fn stop_counting(&self) -> Result<Counts, LoadError> {
        let reply = command(self.control, STOP)?;
        let mut words = reply.split_whitespace().map(str::parse);

        match (words.next(), words.next()) {
            (Some(Ok(allocs)), Some(Ok(bytes))) => Ok(Counts { allocs, bytes }),
            _ => Err(LoadError::Control(reply)),
        }
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        // The server may already have exited, in which case there is nothing left to stop.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn ready(line: &str) -> Option<(SocketAddr, SocketAddr)> {
    let mut words = line.split_whitespace();

    if words.next()? != READY {
        return None;
    }

    Some((words.next()?.parse().ok()?, words.next()?.parse().ok()?))
}

fn command(control: SocketAddr, command: &str) -> Result<String, LoadError> {
    let mut stream = TcpStream::connect(control)?;
    let mut reply = String::new();

    writeln!(stream, "{command}")?;
    BufReader::new(stream).read_line(&mut reply)?;

    Ok(reply.trim().to_owned())
}
