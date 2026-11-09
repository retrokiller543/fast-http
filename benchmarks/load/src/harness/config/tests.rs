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
