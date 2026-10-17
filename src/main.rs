//! CLI entry point: parse, configure, run, and map failures to exit codes.
//!
//! `main` stays thin. Parsing lives in [`boxgen::cli`], configuration in
//! [`boxgen::config`], logging in [`boxgen::logging`], and the exit-code
//! mapping in [`boxgen::error`].

use std::process::ExitCode as ProcessExitCode;

use clap::Parser;

use boxgen::cli::{self, Cli};
use boxgen::config::Config;
use boxgen::error::{BoxgenError, ExitCode};
use boxgen::logging;

fn main() -> ProcessExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            // clap uses 2 for usage errors, but the generator contract reserves
            // 2 for invalid specs. Help and version still exit successfully.
            let code = if error.use_stderr() {
                ExitCode::Failure
            } else {
                ExitCode::Success
            };
            let _ = error.print();
            return ProcessExitCode::from(code.code());
        }
    };

    match run(cli) {
        Ok(()) => ProcessExitCode::SUCCESS,
        Err(error) => {
            eprintln!("boxgen: {error}");
            ProcessExitCode::from(error.exit_code().code())
        }
    }
}

fn run(cli: Cli) -> Result<(), BoxgenError> {
    let config = Config::load()?;
    logging::init(&config)?;
    cli::run(cli, &config)
}
