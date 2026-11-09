//! The `load` run: starts each server, drives it through the phases with `oha` and collects
//! latency, throughput, RSS and allocations.

use std::{
    io,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use self::{
    config::{Config, Framework, Named, Profile, Scenario},
    oha::{Limit, Load, Outcome},
    report::{Allocations, FrameworkReport, Measured, Phase, Report},
    rss::{Memory, Sampler, cpu_per_request},
    server::ServerProcess,
};

pub mod config;
pub mod oha;
pub mod report;
pub mod rss;
pub mod server;

/// Failure of a `load` run.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// `oha` could not be started.
    #[error("could not run oha; install it with `cargo install oha`")]
    OhaMissing(#[source] io::Error),
    /// `oha` exited with an error.
    #[error("oha failed on scenario `{scenario}`: {stderr}")]
    Oha {
        /// Scenario that was running.
        scenario: &'static str,
        /// What oha printed.
        stderr: String,
    },
    /// A server binary could not be started.
    #[error("could not start {}", binary.display())]
    Spawn {
        /// Path of the binary.
        binary: PathBuf,
        /// Why it failed.
        #[source]
        source: io::Error,
    },
    /// A server exited without printing its ready line.
    #[error("{0} exited before it was ready")]
    NotReady(&'static str),
    /// The control socket answered something unexpected.
    #[error("unexpected control reply `{0}`")]
    Control(String),
    /// oha's report or the summary could not be (de)serialized.
    #[error("invalid JSON")]
    Json(#[from] serde_json::Error),
    /// The process table could not be read for RSS sampling.
    #[error("cannot sample memory")]
    Sysinfo(#[from] sysinfo::Error),
    /// Any other I/O failure.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The frameworks in the order repeat `repeat` runs them.
pub fn rotated(frameworks: &[Framework], repeat: u32) -> Vec<Framework> {
    let mut order = frameworks.to_vec();
    let len = order.len();

    if len > 0 {
        order.rotate_left(repeat as usize % len);
    }

    order
}

/// Measures every configured framework, one after the other, for every repeat.
pub fn run(config: &Config) -> Result<Report, LoadError> {
    let mut frameworks = Vec::new();

    for repeat in 0..config.repeat {
        for framework in rotated(&config.frameworks, repeat) {
            frameworks.push(measure(config, framework, repeat)?);
        }
    }

    Ok(Report {
        config: config.clone(),
        frameworks,
    })
}

/// Phases recorded against the time the server became ready.
struct Timeline {
    framework: Framework,
    repeat: u32,
    origin: Instant,
    phases: Vec<Phase>,
}

impl Timeline {
    /// Runs `phase` and records when it started and ended.
    fn record<T>(&mut self, label: String, phase: impl FnOnce() -> T) -> (T, Phase) {
        if self.repeat > 0 {
            eprintln!("[{} #{}] {label}", self.framework.name(), self.repeat);
        } else {
            eprintln!("[{}] {label}", self.framework.name());
        }

        let start_ms = self.elapsed_ms();
        let result = phase();
        let phase = Phase {
            label,
            start_ms,
            end_ms: self.elapsed_ms(),
        };

        self.phases.push(phase.clone());

        (result, phase)
    }

    fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

fn measure(
    config: &Config,
    framework: Framework,
    repeat: u32,
) -> Result<FrameworkReport, LoadError> {
    let server = ServerProcess::spawn(framework, config.workers)?;
    let origin = Instant::now();
    let sampler = Sampler::start(server.pid(), config.sample, origin)?;
    let mut timeline = Timeline {
        framework,
        repeat,
        origin,
        phases: Vec::new(),
    };
    let driven = drive(config, &server, &mut timeline);
    let samples = sampler.finish();

    drop(server);

    let (allocations, mut loads) = driven?;
    let memory = |label: &str| {
        timeline
            .phases
            .iter()
            .find(|phase| phase.label == label)
            .and_then(|phase| Memory::over(&samples, phase.start_ms, phase.end_ms))
    };

    for load in &mut loads {
        load.memory = Memory::over(&samples, load.phase.start_ms, load.phase.end_ms);
        load.cpu_us = cpu_per_request(
            &samples,
            load.phase.start_ms,
            load.phase.end_ms,
            load.outcome.responses,
        );
    }

    Ok(FrameworkReport {
        framework,
        repeat,
        idle: memory("idle"),
        cooldown: memory("cooldown"),
        peak_rss: samples.iter().map(|sample| sample.rss).max().unwrap_or(0),
        allocations,
        loads,
        phases: timeline.phases,
        samples,
    })
}

fn drive(
    config: &Config,
    server: &ServerProcess,
    timeline: &mut Timeline,
) -> Result<(Vec<Allocations>, Vec<Measured>), LoadError> {
    let base = server.base();
    let load = |scenario: Scenario, connections: usize, limit: Limit| {
        oha::run(Load {
            base: &base,
            scenario,
            connections,
            limit,
        })
    };
    let mut allocations = Vec::new();
    let mut loads = Vec::new();

    timeline.record("idle".to_owned(), || thread::sleep(config.idle));

    for &scenario in &config.scenarios {
        let limit = time(config.warmup, None);

        timeline
            .record(format!("warmup/{}", scenario.name()), || {
                load(scenario, config.connections, limit)
            })
            .0?;
    }

    for &scenario in &config.scenarios {
        server.start_counting()?;

        let (outcome, _) = timeline.record(format!("allocs/{}", scenario.name()), || {
            load(scenario, 1, Limit::Requests(config.alloc_requests))
        });
        let counts = server.stop_counting()?;

        allocations.push(Allocations::new(scenario, &outcome?, counts));
    }

    for &profile in &config.profiles {
        for &scenario in &config.scenarios {
            let limit = time(config.duration, config.rate(profile));
            let label = format!("{}/{}", profile.name(), scenario.name());
            let (outcome, phase) =
                timeline.record(label, || load(scenario, config.connections, limit));

            loads.push(measured(profile, scenario, outcome?, phase));
        }
    }

    timeline.record("cooldown".to_owned(), || thread::sleep(config.idle));

    Ok((allocations, loads))
}

fn time(duration: Duration, rate: Option<u32>) -> Limit {
    Limit::Time { duration, rate }
}

fn measured(profile: Profile, scenario: Scenario, outcome: Outcome, phase: Phase) -> Measured {
    Measured {
        profile,
        scenario,
        outcome,
        phase,
        memory: None,
        cpu_us: None,
    }
}

#[cfg(test)]
mod tests;
