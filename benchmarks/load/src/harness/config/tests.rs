use super::*;

fn parse(args: &[&str]) -> Result<Config, ConfigError> {
    Config::parse(args.iter().map(|arg| (*arg).to_owned()))
}

#[test]
fn flags_override_defaults() {
    let config = parse(&[
        "--frameworks",
        "actix,fast-http",
        "--scenarios",
        "create",
        "--duration",
        "3",
        "--medium-rate",
        "5000",
    ])
    .expect("valid flags");

    assert_eq!(config.frameworks, [Framework::Actix, Framework::FastHttp]);
    assert_eq!(config.scenarios, [Scenario::Create]);
    assert_eq!(config.profiles, Profile::ALL);
    assert_eq!(config.duration, Duration::from_secs(3));
    assert_eq!(config.rate(Profile::Medium), Some(5000));
    assert_eq!(config.rate(Profile::Heavy), None);
}

#[test]
fn invalid_arguments_are_rejected() {
    assert_eq!(
        parse(&["--profiles", "light,huge"]).expect_err("unknown profile"),
        ConfigError::UnknownChoice("huge".to_owned())
    );
    assert_eq!(
        parse(&["--workers"]).expect_err("missing value"),
        ConfigError::MissingValue("--workers".to_owned())
    );
    assert_eq!(
        parse(&["--idle", "soon"]).expect_err("not a number"),
        ConfigError::InvalidValue {
            flag: "--idle".to_owned(),
            value: "soon".to_owned()
        }
    );
    assert_eq!(
        parse(&["--rps", "1"]).expect_err("unknown flag"),
        ConfigError::UnknownFlag("--rps".to_owned())
    );
}

#[test]
fn repeat_is_parsed_and_zero_is_rejected() {
    let config = parse(&["--repeat", "3"]).expect("valid");

    assert_eq!(config.repeat, 3);
    assert!(parse(&["--repeat", "0"]).is_err());
}

#[cfg(not(target_os = "linux"))]
#[test]
fn cpu_pinning_is_rejected_off_linux() {
    assert_eq!(
        parse(&["--server-cpus", "0-3"]).unwrap_err(),
        ConfigError::PinningNeedsLinux
    );
    assert_eq!(
        parse(&["--client-cpus", "4-7"]).unwrap_err(),
        ConfigError::PinningNeedsLinux
    );
}

#[cfg(target_os = "linux")]
#[test]
fn cpu_pinning_lists_pass_through_on_linux() {
    let config = parse(&["--server-cpus", "0-3", "--client-cpus", "4-7"]).expect("valid");

    assert_eq!(config.server_cpus.as_deref(), Some("0-3"));
    assert_eq!(config.client_cpus.as_deref(), Some("4-7"));
}
