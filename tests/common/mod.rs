//! Shared scaffolding for the boxgen integration tests.
//!
//! Every integration-test binary compiles this module separately and uses a
//! different subset of it, so dead-code warnings are allowed module-wide.
//! The helpers cover the repeated process-level plumbing: the `boxgen`
//! command, fixture paths, the `BOXGEN_REQUIRE_FREECAD` gate, running the
//! headless `cad/tests/*.py` measurement scripts and parsing their
//! `BOXGEN_JSON:` marker, reading report values, the millimetre assertions,
//! the fake FreeCAD launcher, and the STEP timestamp normaliser.

#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use serde_json::Value;

/// Set to `1` by the gate image to make the FreeCAD checks mandatory.
pub const REQUIRE_ENV: &str = "BOXGEN_REQUIRE_FREECAD";

/// Marker the headless scripts print before their JSON report.
pub const JSON_PREFIX: &str = "BOXGEN_JSON:";

/// Comparison tolerance for exact millimetre values.
pub const EPS: f64 = 1e-6;

/// A `boxgen` process command.
pub fn boxgen() -> Command {
    Command::cargo_bin("boxgen").expect("boxgen binary is built for integration tests")
}

/// The repository root.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A path under `tests/fixtures/`.
pub fn fixture(name: &str) -> PathBuf {
    root().join("tests/fixtures").join(name)
}

/// True when the FreeCAD geometry checks must run; prints a skip otherwise.
pub fn freecad_required() -> bool {
    if std::env::var(REQUIRE_ENV).ok().as_deref() == Some("1") {
        return true;
    }
    eprintln!(
        "skipping FreeCAD geometry check: {REQUIRE_ENV}=1 is only set inside the pinned image"
    );
    false
}

/// Asserts a process succeeded, showing stdout and stderr on failure.
pub fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Runs `cad/tests/<script>` under `freecadcmd` and parses its marker JSON.
pub fn script_report(script: &str) -> Value {
    let path = root().join("cad/tests").join(script);
    let output = Command::new("freecadcmd")
        .arg(&path)
        .current_dir(root())
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "{REQUIRE_ENV}=1 but `freecadcmd {}` could not run: {error}; use `just check-docker`",
                path.display()
            )
        });

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{} failed with {}:\nstdout:\n{stdout}\nstderr:\n{stderr}",
        path.display(),
        output.status
    );

    let line = stdout
        .lines()
        .find_map(|line| line.strip_prefix(JSON_PREFIX))
        .unwrap_or_else(|| {
            panic!(
                "{} printed no {JSON_PREFIX} line:\nstdout:\n{stdout}\nstderr:\n{stderr}",
                path.display()
            )
        });

    serde_json::from_str(line).unwrap_or_else(|error| {
        panic!(
            "{} report is not valid JSON ({error}):\n{line}",
            path.display()
        )
    })
}

/// Runs `cad/tests/step_slices.py` on an exported STEP and parses its JSON.
pub fn slice_step(step: &Path, heights: &str) -> Value {
    let script = root().join("cad/tests/step_slices.py");

    let output = Command::new("freecadcmd")
        .arg(&script)
        .env("BOXGEN_STEP", step)
        .env("BOXGEN_SLICES", heights)
        .current_dir(root())
        .output()
        .expect("freecadcmd runs the slice script");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "slice script failed with {}:\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    let line = stdout
        .lines()
        .find_map(|line| line.strip_prefix(JSON_PREFIX))
        .unwrap_or_else(|| {
            panic!("slice script printed no marker:\nstdout:\n{stdout}\nstderr:\n{stderr}")
        });
    serde_json::from_str(line).unwrap_or_else(|error| panic!("slice report is not JSON: {error}"))
}

/// Reads a nested value by dot-separated keys; numeric keys index arrays.
pub fn value<'a>(report: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(report, |node, key| {
        let child = match (node, key.parse::<usize>()) {
            (Value::Array(_), Ok(index)) => node.get(index),
            _ => node.get(key),
        };
        child.unwrap_or_else(|| panic!("report has no {path:?}"))
    })
}

/// Reads a number by dot-separated keys.
pub fn number(report: &Value, path: &str) -> f64 {
    value(report, path)
        .as_f64()
        .unwrap_or_else(|| panic!("report field {path:?} is not a number"))
}

/// Reads a boolean by dot-separated keys.
pub fn flag(report: &Value, path: &str) -> bool {
    value(report, path)
        .as_bool()
        .unwrap_or_else(|| panic!("report field {path:?} is not a boolean"))
}

/// Reads a list's length by dot-separated keys.
pub fn list_len(report: &Value, path: &str) -> usize {
    value(report, path)
        .as_array()
        .unwrap_or_else(|| panic!("report field {path:?} is not a list"))
        .len()
}

/// Reads a wire bounding box as `[x_min, y_min, x_max, y_max]`.
pub fn wire_box(report: &Value, path: &str) -> [f64; 4] {
    let list = value(report, path)
        .as_array()
        .unwrap_or_else(|| panic!("{path}: not a wire box"));
    assert_eq!(list.len(), 4, "{path}: not a wire box: {list:?}");
    [
        list[0].as_f64().expect("x_min"),
        list[1].as_f64().expect("y_min"),
        list[2].as_f64().expect("x_max"),
        list[3].as_f64().expect("y_max"),
    ]
}

/// Asserts a measured dimension against the pinned value exactly.
pub fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < EPS,
        "{what}: expected {expected} mm, measured {actual} mm"
    );
}

/// Asserts a JSON number list against pinned dimensions.
pub fn assert_close_list(value: &Value, expected: &[f64], what: &str) {
    let list = value
        .as_array()
        .unwrap_or_else(|| panic!("{what}: not a list: {value}"));
    assert_eq!(list.len(), expected.len(), "{what}: wrong length: {list:?}");
    for (item, expected) in list.iter().zip(expected) {
        assert_close(item.as_f64().expect("dimension"), *expected, what);
    }
}

/// Asserts a measured dimension against a pinned value within `tolerance`.
pub fn assert_near(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected {expected} +/- {tolerance} mm, measured {actual} mm"
    );
}

/// Writes an executable shell launcher into `dir`.
pub fn fake_launcher(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("freecadcmd");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fake launcher");
    let mut permissions = fs::metadata(&path)
        .expect("launcher metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("make launcher executable");
    path
}

/// Replaces STEP timestamps so two runs compare byte-for-byte.
///
/// The `FILE_NAME` timestamp is part of the header, not a geometry entity, so
/// normalizing every quoted ISO-8601 date-time token is safe and also covers
/// the header's line wrapping (`FILE_NAME(... ('Author'),(` continues on the
/// next line).
pub fn normalise_step(text: &str) -> String {
    text.lines()
        .map(normalise_quoted_timestamps)
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalise_quoted_timestamps(line: &str) -> String {
    let mut output = String::new();
    let mut rest = line;

    while let Some(quote) = rest.find('\'') {
        let (before, quoted) = rest.split_at(quote + 1);
        output.push_str(before);

        let Some(end) = quoted.find('\'') else {
            output.push_str(quoted);
            return output;
        };
        let (token, after) = quoted.split_at(end);
        if looks_like_timestamp(token) {
            output.push_str("<timestamp>");
        } else {
            output.push_str(token);
        }
        output.push('\'');
        rest = &after[1..];
    }

    output.push_str(rest);
    output
}

fn looks_like_timestamp(token: &str) -> bool {
    let bytes = token.as_bytes();
    token.len() == 19
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
}
