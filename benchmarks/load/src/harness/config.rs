//! What a run measures, parsed from the `load` command line.

use std::{env::consts::EXE_SUFFIX, path::PathBuf, time::Duration};

use serde::Serialize;

/// Command-line help printed with every argument error.
pub const USAGE: &str = "\
usage: load [options]
  --frameworks LIST     fast-http,axum,actix (default: all)
  --profiles LIST       light,medium,heavy (default: all)
  --scenarios LIST      plaintext,json,create (default: all)
  --workers N           server worker threads (default: half the cores)
  --connections N       concurrent connections (default: 64)
  --duration SECS       length of each measured phase (default: 10)
  --idle SECS           idle phase before and after the load (default: 10)
  --warmup SECS         unmeasured load per scenario before measuring (default: 2)
  --light-rate RPS      fixed request rate of the light profile (default: 1000)
  --medium-rate RPS     fixed request rate of the medium profile (default: 20000)
  --alloc-requests N    requests per allocation-counting pass (default: 10000)
  --repeat N            runs of the whole matrix, rotating the framework order (default: 1)
  --output DIR          where result directories are written (default: results)";

/// A finite choice named on the command line.
pub trait Named: Copy + 'static {
    /// Every value, in reporting order.
    const ALL: &'static [Self];

    /// Command-line and report name.
    fn name(self) -> &'static str;
}

/// Framework under test; each runs as its own server binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Framework {
    /// fast-http with the hyper engine.
    FastHttp,
    /// axum over hyper.
    Axum,
    /// actix-web with a single-threaded runtime per worker.
    Actix,
}

impl Framework {
    /// File name of the server binary, next to the `load` binary.
    pub fn binary(self) -> String {
        format!("serve-{}{EXE_SUFFIX}", self.name())
    }
}

impl Named for Framework {
    const ALL: &'static [Self] = &[Self::FastHttp, Self::Axum, Self::Actix];

    fn name(self) -> &'static str {
        match self {
            Self::FastHttp => "fast-http",
            Self::Axum => "axum",
            Self::Actix => "actix",
        }
    }
}

/// Load level of a measured phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    /// Fixed low rate, open loop.
    Light,
    /// Fixed higher rate, open loop.
    Medium,
    /// As fast as the connections allow, closed loop.
    Heavy,
}

impl Named for Profile {
    const ALL: &'static [Self] = &[Self::Light, Self::Medium, Self::Heavy];

    fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Medium => "medium",
            Self::Heavy => "heavy",
        }
    }
}

/// Request sent during a phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scenario {
    /// `GET /health`, a static text body.
    Plaintext,
    /// `GET /users/{id}`, a path parameter and a serialized JSON body.
    Json,
    /// `POST /users`, a deserialized JSON body serialized back.
    Create,
}

impl Scenario {
    /// Request path.
    pub fn path(self) -> &'static str {
        match self {
            Self::Plaintext => "/health",
            Self::Json => "/users/42",
            Self::Create => "/users",
        }
    }

    /// JSON request body; requests with a body are sent as `POST`.
    pub fn body(self) -> Option<&'static str> {
        match self {
            Self::Plaintext | Self::Json => None,
            Self::Create => {
                Some(r#"{"name":"Ada Lovelace","email":"ada@example.com","tags":["admin","math"]}"#)
            }
        }
    }
}

impl Named for Scenario {
    const ALL: &'static [Self] = &[Self::Plaintext, Self::Json, Self::Create];

    fn name(self) -> &'static str {
        match self {
            Self::Plaintext => "plaintext",
            Self::Json => "json",
            Self::Create => "create",
        }
    }
}

/// Settings of one `load` run.
#[derive(Debug, Clone, Serialize)]
pub struct Config {
    /// Frameworks to measure, in order.
    pub frameworks: Vec<Framework>,
    /// Measured load profiles.
    pub profiles: Vec<Profile>,
    /// Requests measured under every profile.
    pub scenarios: Vec<Scenario>,
    /// Server worker threads.
    pub workers: usize,
    /// Concurrent client connections.
    pub connections: usize,
    /// Length of each measured phase.
    pub duration: Duration,
    /// Length of the idle phases before and after the load.
    pub idle: Duration,
    /// Unmeasured load per scenario before the measured phases.
    pub warmup: Duration,
    /// Request rate of [`Profile::Light`].
    pub light_rate: u32,
    /// Request rate of [`Profile::Medium`].
    pub medium_rate: u32,
    /// Requests per allocation-counting pass.
    pub alloc_requests: u32,
    /// Times the whole matrix runs, each time starting with the next framework.
    pub repeat: u32,
    /// Interval between RSS samples.
    pub sample: Duration,
    /// Directory the result directories are written to.
    pub output: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map_or(2, usize::from);

        Self {
            frameworks: Framework::ALL.to_vec(),
            profiles: Profile::ALL.to_vec(),
            scenarios: Scenario::ALL.to_vec(),
            workers: (cores / 2).max(1),
            connections: 64,
            duration: Duration::from_secs(10),
            idle: Duration::from_secs(10),
            warmup: Duration::from_secs(2),
            light_rate: 1_000,
            medium_rate: 20_000,
            alloc_requests: 10_000,
            repeat: 1,
            sample: Duration::from_millis(100),
            output: PathBuf::from("results"),
        }
    }
}

impl Config {
    /// Parses `--flag value` pairs over the defaults.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, ConfigError> {
        let mut config = Self::default();
        let mut args = args.into_iter();

        while let Some(flag) = args.next() {
            let value = args
                .next()
                .ok_or_else(|| ConfigError::MissingValue(flag.clone()))?;

            match flag.as_str() {
                "--frameworks" => config.frameworks = list(&value)?,
                "--profiles" => config.profiles = list(&value)?,
                "--scenarios" => config.scenarios = list(&value)?,
                "--workers" => config.workers = number(&flag, &value)?,
                "--connections" => config.connections = number(&flag, &value)?,
                "--duration" => config.duration = seconds(&flag, &value)?,
                "--idle" => config.idle = seconds(&flag, &value)?,
                "--warmup" => config.warmup = seconds(&flag, &value)?,
                "--light-rate" => config.light_rate = number(&flag, &value)?,
                "--medium-rate" => config.medium_rate = number(&flag, &value)?,
                "--alloc-requests" => config.alloc_requests = number(&flag, &value)?,
                "--repeat" => {
                    config.repeat = number(&flag, &value)
                        .ok()
                        .filter(|repeat| *repeat > 0)
                        .ok_or(ConfigError::InvalidValue { flag, value })?;
                }
                "--output" => config.output = PathBuf::from(value),
                _ => return Err(ConfigError::UnknownFlag(flag)),
            }
        }

        Ok(config)
    }

    /// Fixed request rate of `profile`; `None` means as fast as possible.
    pub fn rate(&self, profile: Profile) -> Option<u32> {
        match profile {
            Profile::Light => Some(self.light_rate),
            Profile::Medium => Some(self.medium_rate),
            Profile::Heavy => None,
        }
    }
}

/// Invalid `load` command line.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// The flag is not one of [`USAGE`].
    #[error("unknown flag `{0}`")]
    UnknownFlag(String),
    /// The flag was last and has no value.
    #[error("`{0}` needs a value")]
    MissingValue(String),
    /// The value of a flag does not parse.
    #[error("invalid value `{value}` for `{flag}`")]
    InvalidValue {
        /// The flag.
        flag: String,
        /// Its value.
        value: String,
    },
    /// A list names an unknown choice.
    #[error("unknown choice `{0}`")]
    UnknownChoice(String),
}

fn list<T: Named>(value: &str) -> Result<Vec<T>, ConfigError> {
    value
        .split(',')
        .map(|name| {
            T::ALL
                .iter()
                .copied()
                .find(|choice| choice.name() == name)
                .ok_or_else(|| ConfigError::UnknownChoice(name.to_owned()))
        })
        .collect()
}

fn number<T: std::str::FromStr>(flag: &str, value: &str) -> Result<T, ConfigError> {
    value.parse().map_err(|_| ConfigError::InvalidValue {
        flag: flag.to_owned(),
        value: value.to_owned(),
    })
}

fn seconds(flag: &str, value: &str) -> Result<Duration, ConfigError> {
    number(flag, value).map(Duration::from_secs)
}

#[cfg(test)]
mod tests;
