//! Results of a run and the files they are written to: `summary.json`, `summary.md` and
//! `rss.csv` (the RSS timeline, labelled by phase).

use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use crate::serve::alloc::Counts;

use super::{
    LoadError,
    config::{Config, Framework, Named, Profile, Scenario},
    oha::Outcome,
    rss::{Memory, Sample},
};

/// Everything one run measured.
#[derive(Debug, Serialize)]
pub struct Report {
    /// Settings the run used.
    pub config: Config,
    /// One entry per framework, in run order.
    pub frameworks: Vec<FrameworkReport>,
}

/// Measurements of one framework.
#[derive(Debug, Serialize)]
pub struct FrameworkReport {
    /// The framework.
    pub framework: Framework,
    /// RSS while idle right after startup.
    pub idle: Option<Memory>,
    /// RSS while idle after all load phases.
    pub cooldown: Option<Memory>,
    /// Highest RSS seen during the whole run.
    pub peak_rss: u64,
    /// Allocation-counting passes, one per scenario.
    pub allocations: Vec<Allocations>,
    /// Measured load phases.
    pub loads: Vec<Measured>,
    /// Every phase, for reading the RSS timeline.
    pub phases: Vec<Phase>,
    /// RSS timeline; written to `rss.csv` instead of the summary.
    #[serde(skip)]
    pub samples: Vec<Sample>,
}

/// A labelled span of the run, in milliseconds since the server became ready.
#[derive(Debug, Clone, Serialize)]
pub struct Phase {
    /// Phase name, such as `heavy/json`.
    pub label: String,
    /// Start.
    pub start_ms: u64,
    /// End.
    pub end_ms: u64,
}

/// Server-side allocations per request for one scenario.
#[derive(Debug, Clone, Serialize)]
pub struct Allocations {
    /// The scenario.
    pub scenario: Scenario,
    /// Responses the pass received.
    pub responses: u64,
    /// Allocations per response, connection handling and runtime included.
    pub allocs_per_request: f64,
    /// Bytes allocated per response.
    pub bytes_per_request: f64,
}

impl Allocations {
    /// Divides the server's counts by the responses of the pass.
    pub fn new(scenario: Scenario, outcome: &Outcome, counts: Counts) -> Self {
        let responses = outcome.responses.max(1) as f64;

        Self {
            scenario,
            responses: outcome.responses,
            allocs_per_request: counts.allocs as f64 / responses,
            bytes_per_request: counts.bytes as f64 / responses,
        }
    }
}

/// One measured load phase.
#[derive(Debug, Clone, Serialize)]
pub struct Measured {
    /// Load level.
    pub profile: Profile,
    /// Request sent.
    pub scenario: Scenario,
    /// Client-side result.
    pub outcome: Outcome,
    /// When it ran.
    pub phase: Phase,
    /// Server RSS during the phase.
    pub memory: Option<Memory>,
}

/// Writes the report to a new `<output>/<unix seconds>` directory and returns it.
pub fn write(report: &Report, output: &Path) -> Result<PathBuf, LoadError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let dir = output.join(stamp.to_string());

    fs::create_dir_all(&dir)?;
    serde_json::to_writer_pretty(
        BufWriter::new(File::create(dir.join("summary.json"))?),
        report,
    )?;
    write_rss(report, &dir.join("rss.csv"))?;
    fs::write(dir.join("summary.md"), markdown(report))?;

    Ok(dir)
}

fn write_rss(report: &Report, path: &Path) -> Result<(), LoadError> {
    let mut csv = BufWriter::new(File::create(path)?);

    writeln!(csv, "framework,phase,at_ms,rss_bytes")?;

    for framework in &report.frameworks {
        for sample in &framework.samples {
            let phase = framework
                .phases
                .iter()
                .find(|phase| (phase.start_ms..=phase.end_ms).contains(&sample.at_ms))
                .map_or("between", |phase| phase.label.as_str());

            writeln!(
                csv,
                "{},{phase},{},{}",
                framework.framework.name(),
                sample.at_ms,
                sample.rss
            )?;
        }
    }

    csv.flush()?;

    Ok(())
}

/// Markdown tables comparing the frameworks.
pub fn markdown(report: &Report) -> String {
    let mut out = String::new();

    out.push_str("## Latency and throughput\n\n");
    row(
        &mut out,
        &[
            "profile",
            "scenario",
            "framework",
            "req/s",
            "p50 ms",
            "p90 ms",
            "p99 ms",
            "p99.9 ms",
            "non-2xx",
            "errors",
            "aborted",
            "mean RSS",
            "peak RSS",
        ],
    );
    separator(&mut out, 13);

    for &profile in &report.config.profiles {
        for &scenario in &report.config.scenarios {
            for framework in &report.frameworks {
                let Some(load) = framework
                    .loads
                    .iter()
                    .find(|load| load.profile == profile && load.scenario == scenario)
                else {
                    continue;
                };
                let outcome = &load.outcome;

                row(
                    &mut out,
                    &[
                        profile.name(),
                        scenario.name(),
                        framework.framework.name(),
                        &format!("{:.0}", outcome.throughput),
                        &millis(outcome.latency.p50),
                        &millis(outcome.latency.p90),
                        &millis(outcome.latency.p99),
                        &millis(outcome.latency.p999),
                        &outcome.unexpected.to_string(),
                        &outcome.errors.to_string(),
                        &outcome.aborted.to_string(),
                        &load.memory.map_or_else(|| "-".to_owned(), |m| mib(m.mean)),
                        &load.memory.map_or_else(|| "-".to_owned(), |m| mib(m.peak)),
                    ],
                );
            }
        }
    }

    out.push_str("\n## Memory\n\n");
    row(
        &mut out,
        &[
            "framework",
            "idle RSS (start)",
            "idle RSS (after load)",
            "peak RSS",
        ],
    );
    separator(&mut out, 4);

    for framework in &report.frameworks {
        row(
            &mut out,
            &[
                framework.framework.name(),
                &framework
                    .idle
                    .map_or_else(|| "-".to_owned(), |m| mib(m.mean)),
                &framework
                    .cooldown
                    .map_or_else(|| "-".to_owned(), |m| mib(m.mean)),
                &mib(framework.peak_rss),
            ],
        );
    }

    out.push_str("\n## Server allocations per request\n\n");
    row(&mut out, &["scenario", "framework", "allocations", "bytes"]);
    separator(&mut out, 4);

    for &scenario in &report.config.scenarios {
        for framework in &report.frameworks {
            if let Some(allocs) = framework
                .allocations
                .iter()
                .find(|allocs| allocs.scenario == scenario)
            {
                row(
                    &mut out,
                    &[
                        scenario.name(),
                        framework.framework.name(),
                        &format!("{:.1}", allocs.allocs_per_request),
                        &format!("{:.0}", allocs.bytes_per_request),
                    ],
                );
            }
        }
    }

    out
}

fn row(out: &mut String, cells: &[&str]) {
    out.push('|');

    for cell in cells {
        out.push(' ');
        out.push_str(cell);
        out.push_str(" |");
    }

    out.push('\n');
}

fn separator(out: &mut String, columns: usize) {
    row(out, &vec!["---"; columns]);
}

fn millis(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |ms| format!("{ms:.3}"))
}

fn mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}
