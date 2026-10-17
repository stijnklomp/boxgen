//! CLI entry point: parse, configure, run, and map failures to exit codes.
//!
//! `main` stays thin. Parsing lives in [`boxgen::cli`], configuration in
//! [`boxgen::config`], logging in [`boxgen::logging`], and the exit-code
//! mapping in [`boxgen::error`].

use std::process::ExitCode as ProcessExitCode;

use clap::Parser;

use boxgen::cli::{self, Cli};
use boxgen::config::Config;
use boxgen::error::BoxgenError;
use boxgen::logging;

fn main() -> ProcessExitCode {
    match run() {
        Ok(()) => ProcessExitCode::SUCCESS,
        Err(error) => {
            eprintln!("boxgen: {error}");
            ProcessExitCode::from(error.exit_code().code())
        }
    }
}

fn run() -> Result<(), BoxgenError> {
    let cli = Cli::parse();
    let config = Config::load()?;
    logging::init(&config)?;
    cli::run(cli, &config)
}
