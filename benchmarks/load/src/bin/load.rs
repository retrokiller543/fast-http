//! Drives every `serve-*` binary with `oha` and writes the comparison. Run it through
//! `just load`, which builds the servers first.

use std::{error::Error, process::ExitCode};

use fast_http_load::harness::{
    self,
    config::{Config, USAGE},
    report,
};

fn main() -> ExitCode {
    let config = match Config::parse(std::env::args().skip(1)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}\n\n{USAGE}");

            return ExitCode::from(2);
        }
    };
    let written = harness::run(&config).and_then(|report| {
        let dir = report::write(&report, &config.output)?;

        println!("{}", report::markdown(&report));

        Ok(dir)
    });

    match written {
        Ok(dir) => {
            println!("results written to {}", dir.display());

            ExitCode::SUCCESS
        }
        Err(error) => {
            let mut source: Option<&dyn Error> = Some(&error);

            eprint!("load failed");

            while let Some(cause) = source {
                eprint!(": {cause}");
                source = cause.source();
            }

            eprintln!();

            ExitCode::FAILURE
        }
    }
}
