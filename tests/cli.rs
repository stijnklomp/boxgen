//! CLI contract tests: help text and the process-level error surface.
//!
//! Later tickets add command behaviour; these tests pin the command names and
//! the exit codes the binary exposes before any command does real work.

mod common;

use predicates::prelude::*;

/// The five commands fixed by the generator contract (docs/generator.md §5).
const CONTRACT_COMMANDS: [&str; 5] = ["validate", "build", "divide", "index", "coupon"];

#[test]
fn help_lists_every_contract_command() {
    let mut command = common::boxgen();
    command
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: boxgen"))
        .stdout(predicate::str::contains("validate"))
        .stdout(predicate::str::contains("build"))
        .stdout(predicate::str::contains("divide"))
        .stdout(predicate::str::contains("index"))
        .stdout(predicate::str::contains("coupon"));
}

#[test]
fn every_contract_command_has_help() {
    for name in CONTRACT_COMMANDS {
        let mut command = common::boxgen();
        command
            .args([name, "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Usage: boxgen"));
    }
}

#[test]
fn invalid_configuration_exits_with_the_environment_code() {
    let mut command = common::boxgen();
    command
        .env("BOXGEN_LOG", "verbose")
        .arg("index")
        .assert()
        .failure()
        .code(4)
        .stderr(predicate::str::contains("BOXGEN_LOG"));
}

#[test]
fn usage_errors_do_not_reuse_the_invalid_spec_exit_code() {
    let mut command = common::boxgen();
    command
        .arg("validate")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("required"));
}
