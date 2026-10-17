//! CLI contract tests: help text and the process-level error surface.
//!
//! Later tickets add command behaviour; these tests pin the command names and
//! the exit codes the binary exposes before any command does real work.

use assert_cmd::Command;
use predicates::prelude::*;

/// The five commands fixed by the generator contract (docs/generator.md §5).
const CONTRACT_COMMANDS: [&str; 5] = ["validate", "build", "divide", "index", "coupon"];

fn boxgen() -> Command {
    Command::cargo_bin("boxgen").expect("boxgen binary is built for integration tests")
}

/// Minimal arguments that satisfy each command's clap definition. The stubs do
/// not read the spec, so the path is intentionally fictional.
fn stub_args(name: &str) -> Vec<&str> {
    match name {
        "validate" | "build" | "divide" => vec![name, "boxes/example/spec.yaml"],
        _ => vec![name],
    }
}

#[test]
fn help_lists_every_contract_command() {
    let mut command = boxgen();
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
        let mut command = boxgen();
        command
            .args([name, "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("Usage: boxgen"));
    }
}

#[test]
fn unimplemented_commands_fail_clearly_with_exit_code_one() {
    for name in CONTRACT_COMMANDS {
        let mut command = boxgen();
        command
            .args(stub_args(name))
            .assert()
            .failure()
            .code(1)
            .stderr(predicate::str::contains("not implemented"));
    }
}

#[test]
fn invalid_configuration_exits_with_the_environment_code() {
    let mut command = boxgen();
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
    let mut command = boxgen();
    command
        .arg("validate")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("required"));
}
