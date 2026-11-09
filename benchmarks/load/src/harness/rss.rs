//! Resident set size of a server process, sampled on a background thread.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use super::LoadError;

/// One RSS reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Sample {
    /// Milliseconds since the server started.
    pub at_ms: u64,
    /// Resident set size in bytes.
    pub rss: u64,
}

/// Mean and peak RSS over a span of time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Memory {
    /// Mean RSS in bytes.
    pub mean: u64,
    /// Highest RSS in bytes.
    pub peak: u64,
}

impl Memory {
    /// Summarises the samples taken in `[start_ms, end_ms]`; `None` if there are none.
    pub fn over(samples: &[Sample], start_ms: u64, end_ms: u64) -> Option<Self> {
        let within = samples
            .iter()
            .filter(|sample| (start_ms..=end_ms).contains(&sample.at_ms));
        let (count, total, peak) = within.fold((0_u64, 0_u64, 0_u64), |(count, total, peak), s| {
            (count + 1, total + s.rss, peak.max(s.rss))
        });

        (count > 0).then(|| Self {
            mean: total / count,
            peak,
        })
    }
}

/// Background thread sampling one process until [`finish`](Self::finish).
pub struct Sampler {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Vec<Sample>>,
}

impl Sampler {
    /// Samples process `pid` every `every`, timed from `origin`.
    pub fn start(pid: u32, every: Duration, origin: Instant) -> Result<Self, LoadError> {
        let system = System::new()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("load-rss".to_owned())
            .spawn(move || sample(system, Pid::from_u32(pid), every, origin, &stopping))?;

        Ok(Self { stop, thread })
    }

    /// Stops sampling and returns the samples in time order.
    pub fn finish(self) -> Vec<Sample> {
        self.stop.store(true, Relaxed);

        self.thread.join().unwrap_or_default()
    }
}

fn sample(
    mut system: System,
    pid: Pid,
    every: Duration,
    origin: Instant,
    stop: &AtomicBool,
) -> Vec<Sample> {
    let mut samples = Vec::new();
    let kind = ProcessRefreshKind::nothing().with_memory();

    while !stop.load(Relaxed) {
        system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, kind);

        if let Some(process) = system.process(pid) {
            samples.push(Sample {
                at_ms: u64::try_from(origin.elapsed().as_millis()).unwrap_or(u64::MAX),
                rss: process.memory(),
            });
        }

        thread::sleep(every);
    }

    samples
}

#[cfg(test)]
mod tests;
