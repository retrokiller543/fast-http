//! Runs `oha` and reads its JSON report.

use std::{collections::BTreeMap, process::Command, time::Duration};

use serde::{Deserialize, Serialize};

use super::{
    LoadError,
    config::{Named, Scenario},
};

/// How long a run lasts.
#[derive(Debug, Clone, Copy)]
pub enum Limit {
    /// Runs for a duration, at a fixed rate (open loop, latency corrected for coordinated
    /// omission) or as fast as the connections allow.
    Time {
        /// Run length.
        duration: Duration,
        /// Requests per second over all connections.
        rate: Option<u32>,
    },
    /// Sends exactly this many requests.
    Requests(u32),
}

/// One `oha` invocation.
#[derive(Debug, Clone, Copy)]
pub struct Load<'a> {
    /// Server base address, such as `http://127.0.0.1:4000`.
    pub base: &'a str,
    /// Request to send.
    pub scenario: Scenario,
    /// Concurrent connections.
    pub connections: usize,
    /// Length of the run.
    pub limit: Limit,
    /// CPUs to pin `oha` to with `taskset -c`; unpinned when `None`.
    pub cpus: Option<&'a str>,
}

/// Client-side result of a run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outcome {
    /// Responses received, any status.
    pub responses: u64,
    /// Responses with a status other than 2xx.
    pub unexpected: u64,
    /// Requests that failed without a response.
    pub errors: u64,
    /// Requests still in flight when a timed run ended; not a server failure.
    pub aborted: u64,
    /// Responses per second.
    pub throughput: f64,
    /// Latency percentiles in milliseconds; `None` without any response.
    pub latency: Latency,
}

/// Latency percentiles in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Latency {
    /// Median.
    pub p50: Option<f64>,
    /// 90th percentile.
    pub p90: Option<f64>,
    /// 99th percentile.
    pub p99: Option<f64>,
    /// 99.9th percentile.
    pub p999: Option<f64>,
}

/// The part of oha's JSON report the harness reads.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    summary: Summary,
    latency_percentiles: Percentiles,
    status_code_distribution: BTreeMap<String, u64>,
    error_distribution: BTreeMap<String, u64>,
}

/// oha's summary block.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    requests_per_sec: f64,
}

/// oha's latency percentiles, in seconds.
#[derive(Deserialize)]
struct Percentiles {
    p50: Option<f64>,
    p90: Option<f64>,
    p99: Option<f64>,
    #[serde(rename = "p99.9")]
    p999: Option<f64>,
}

/// Runs `oha` for `load` and waits for its report.
pub fn run(load: Load<'_>) -> Result<Outcome, LoadError> {
    let mut command = match load.cpus {
        Some(cpus) if cfg!(target_os = "linux") => {
            let mut command = Command::new("taskset");

            command.args(["-c", cpus, "oha"]);

            command
        }
        _ => Command::new("oha"),
    };

    command
        .args(["--no-tui", "--output-format", "json", "-c"])
        .arg(load.connections.to_string());

    match load.limit {
        Limit::Time { duration, rate } => {
            command
                .arg("-z")
                .arg(format!("{}s", duration.as_secs().max(1)));

            if let Some(rate) = rate {
                command
                    .arg("-q")
                    .arg(rate.to_string())
                    .arg("--latency-correction");
            }
        }
        Limit::Requests(requests) => {
            command.arg("-n").arg(requests.to_string());
        }
    }

    if let Some(body) = load.scenario.body() {
        command.args(["-m", "POST", "-T", "application/json", "-d", body]);
    }

    let output = command
        .arg(format!("{}{}", load.base, load.scenario.path()))
        .output()
        .map_err(LoadError::OhaMissing)?;

    if !output.status.success() {
        return Err(LoadError::Oha {
            scenario: load.scenario.name(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    parse(&output.stdout)
}

/// oha's error for requests cut off by the end of a `-z` run.
const DEADLINE: &str = "aborted due to deadline";

/// Reads an oha JSON report.
pub fn parse(json: &[u8]) -> Result<Outcome, LoadError> {
    let report: Report = serde_json::from_slice(json)?;
    let millis = |seconds: Option<f64>| seconds.map(|seconds| seconds * 1_000.0);
    let unexpected = report
        .status_code_distribution
        .iter()
        .filter(|(status, _)| !status.starts_with('2'))
        .map(|(_, count)| count)
        .sum();
    let aborted = report
        .error_distribution
        .iter()
        .filter(|(error, _)| error.as_str() == DEADLINE)
        .map(|(_, count)| count)
        .sum();

    Ok(Outcome {
        responses: report.status_code_distribution.values().sum(),
        unexpected,
        errors: report.error_distribution.values().sum::<u64>() - aborted,
        aborted,
        throughput: report.summary.requests_per_sec,
        latency: Latency {
            p50: millis(report.latency_percentiles.p50),
            p90: millis(report.latency_percentiles.p90),
            p99: millis(report.latency_percentiles.p99),
            p999: millis(report.latency_percentiles.p999),
        },
    })
}

#[cfg(test)]
mod tests;
