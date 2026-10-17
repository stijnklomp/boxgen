//! Stderr logging for a local CLI.
//!
//! The server template logged JSON to stdout because the HTTP server owned
//! stdout. boxgen reserves stdout for command reports (`validate`, `index`) and
//! sends diagnostics to stderr, so the subscriber is human-readable and
//! stderr-only. The level comes from [`Config::log_level`].

use std::io;

use crate::config::Config;
use crate::error::BoxgenError;

/// Installs the global tracing subscriber once per process.
pub fn init(config: &Config) -> Result<(), BoxgenError> {
    tracing_subscriber::fmt()
        .with_max_level(config.log_level)
        .with_writer(io::stderr)
        .with_target(false)
        .without_time()
        .try_init()
        .map_err(|error| BoxgenError::Environment {
            message: format!("failed to install the logger: {error}"),
        })
}
