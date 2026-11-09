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
    /// One entry per framework per repeat, in run order.
    pub frameworks: Vec<FrameworkReport>,
}

/// Measurements of one framework.
#[derive(Debug, Serialize)]
pub struct FrameworkReport {
    /// The framework.
    pub framework: Framework,
    /// Zero-based repeat of the whole matrix this run belongs to.
    pub repeat: u32,
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
    /// Server CPU microseconds per response.
    pub cpu_us: Option<f64>,
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

    writeln!(csv, "framework,repeat,phase,at_ms,rss_bytes,cpu_ms")?;

    for framework in &report.frameworks {
        for sample in &framework.samples {
            let phase = framework
                .phases
                .iter()
                .find(|phase| (phase.start_ms..=phase.end_ms).contains(&sample.at_ms))
                .map_or("between", |phase| phase.label.as_str());

            writeln!(
                csv,
                "{},{},{phase},{},{},{}",
                framework.framework.name(),
                framework.repeat,
                sample.at_ms,
                sample.rss,
                sample.cpu_ms
            )?;
        }
    }

    csv.flush()?;

    Ok(())
}

/// Median and range of one measurement over the repeats.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Spread {
    /// Median value.
    pub median: f64,
    /// Smallest value.
    pub min: f64,
    /// Largest value.
    pub max: f64,
}

fn spread(values: &mut [f64]) -> Option<Spread> {
    values.sort_by(f64::total_cmp);

    let (min, max) = (*values.first()?, *values.last()?);
    let middle = values.len() / 2;
    let median = if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    };

    Some(Spread { median, min, max })
}

/// The runs of each framework across the repeats, in first-run order.
fn by_framework(report: &Report) -> Vec<(Framework, Vec<&FrameworkReport>)> {
    let mut groups: Vec<(Framework, Vec<&FrameworkReport>)> = Vec::new();

    for run in &report.frameworks {
        match groups
            .iter_mut()
            .find(|(framework, _)| *framework == run.framework)
        {
            Some((_, runs)) => runs.push(run),
            None => groups.push((run.framework, vec![run])),
        }
    }

    groups
}

/// A table cell: the median, with the range appended when `range` and there are several values.
fn cell(mut values: Vec<f64>, precision: usize, range: bool) -> String {
    let many = values.len() > 1;

    match spread(&mut values) {
        None => "-".to_owned(),
        Some(s) if range && many => format!(
            "{:.precision$} ({:.precision$}–{:.precision$})",
            s.median, s.min, s.max
        ),
        Some(s) => format!("{:.precision$}", s.median),
    }
}

fn mib_cell(mut values: Vec<f64>) -> String {
    match spread(&mut values) {
        Some(s) => format!("{:.1} MiB", s.median / (1024.0 * 1024.0)),
        None => "-".to_owned(),
    }
}

/// Markdown tables comparing the frameworks; repeats are summarised by their median.
pub fn markdown(report: &Report) -> String {
    let mut out = String::new();
    let groups = by_framework(report);
    let ranges = report.config.repeat > 1;

    out.push_str("## Latency and throughput\n\n");
    row(
        &mut out,
        &[
            "profile",
            "scenario",
            "framework",
            "req/s",
            "cpu µs/req",
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
    separator(&mut out, 14);

    for &profile in &report.config.profiles {
        for &scenario in &report.config.scenarios {
            for (framework, runs) in &groups {
                let loads: Vec<&Measured> = runs
                    .iter()
                    .filter_map(|run| {
                        run.loads
                            .iter()
                            .find(|load| load.profile == profile && load.scenario == scenario)
                    })
                    .collect();

                if loads.is_empty() {
                    continue;
                }

                let all = |value: fn(&Measured) -> f64| loads.iter().map(|l| value(l)).collect();
                let some = |value: fn(&Measured) -> Option<f64>| {
                    loads.iter().filter_map(|l| value(l)).collect()
                };

                row(
                    &mut out,
                    &[
                        profile.name(),
                        scenario.name(),
                        framework.name(),
                        &cell(all(|l| l.outcome.throughput), 0, ranges),
                        &cell(some(|l| l.cpu_us), 1, ranges),
                        &cell(some(|l| l.outcome.latency.p50), 3, false),
                        &cell(some(|l| l.outcome.latency.p90), 3, false),
                        &cell(some(|l| l.outcome.latency.p99), 3, false),
                        &cell(some(|l| l.outcome.latency.p999), 3, false),
                        &cell(all(|l| l.outcome.unexpected as f64), 0, false),
                        &cell(all(|l| l.outcome.errors as f64), 0, false),
                        &cell(all(|l| l.outcome.aborted as f64), 0, false),
                        &mib_cell(some(|l| l.memory.map(|m| m.mean as f64))),
                        &mib_cell(some(|l| l.memory.map(|m| m.peak as f64))),
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

    for (framework, runs) in &groups {
        let idle = runs.iter().filter_map(|r| r.idle.map(|m| m.mean as f64));
        let cooldown = runs
            .iter()
            .filter_map(|r| r.cooldown.map(|m| m.mean as f64));
        let peak = runs.iter().map(|r| r.peak_rss as f64);

        row(
            &mut out,
            &[
                framework.name(),
                &mib_cell(idle.collect()),
                &mib_cell(cooldown.collect()),
                &mib_cell(peak.collect()),
            ],
        );
    }

    out.push_str("\n## Server allocations per request\n\n");
    row(&mut out, &["scenario", "framework", "allocations", "bytes"]);
    separator(&mut out, 4);

    for &scenario in &report.config.scenarios {
        for (framework, runs) in &groups {
            let counts: Vec<&Allocations> = runs
                .iter()
                .filter_map(|run| run.allocations.iter().find(|a| a.scenario == scenario))
                .collect();

            if counts.is_empty() {
                continue;
            }

            row(
                &mut out,
                &[
                    scenario.name(),
                    framework.name(),
                    &cell(
                        counts.iter().map(|a| a.allocs_per_request).collect(),
                        1,
                        false,
                    ),
                    &cell(
                        counts.iter().map(|a| a.bytes_per_request).collect(),
                        0,
                        false,
                    ),
                ],
            );
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

#[cfg(test)]
mod tests;
