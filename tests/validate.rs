//! `boxgen validate` process behaviour: report, warnings, and exit code 2.
//!
//! Contract: `docs/spec/docs/generator.md` §3, §4, §5 and §7. A valid spec prints a
//! human-readable report to stdout and exits 0; an invalid or unreadable spec
//! prints precise diagnostics to stderr and exits 2. Warnings never fail.

mod common;

use assert_cmd::assert::Assert;
use predicates::prelude::*;

fn validate(path: &str) -> Assert {
    let mut command = common::boxgen();
    command.arg("validate").arg(common::fixture(path)).assert()
}

#[test]
fn a_valid_minimal_spec_prints_a_report_and_exits_zero() {
    validate("minimal.yaml")
        .success()
        .code(0)
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("id: 1x1-3u-bits-v1"))
        .stdout(predicate::str::contains(
            "footprint: 1 x 1 (outer 41.5 x 41.5 mm)",
        ))
        .stdout(predicate::str::contains("height: 3U (25.4 mm total)"))
        .stdout(predicate::str::contains("interior: 37.5 x 37.5 x 15.45 mm"))
        .stdout(predicate::str::contains("lid: no"))
        .stdout(predicate::str::contains("compartments: 1"))
        .stdout(predicate::str::contains("Bits"))
        .stdout(predicate::str::contains("estimated mass:"))
        .stdout(predicate::str::contains("estimated print time:"))
        .stdout(predicate::str::contains("warnings: none"));
}

#[test]
fn a_valid_4x4_spec_lists_every_compartment() {
    validate("4x4-6u.yaml")
        .success()
        .code(0)
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("compartments: 4"))
        .stdout(predicate::str::contains("USB-C short"))
        .stdout(predicate::str::contains("USB-C long"))
        .stdout(predicate::str::contains("Power"))
        .stdout(predicate::str::contains("Adapters"))
        .stdout(predicate::str::contains("warnings: none"));
}

#[test]
fn the_golden_tree_fixture_is_valid() {
    validate("4x4-6u-tree.yaml")
        .success()
        .code(0)
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("compartments: 4"))
        .stdout(predicate::str::contains("warnings: none"));
}

/// The deepest legal corner nesting is valid but warns: its corner leaf is
/// 12.5 mm, below the 15 mm threshold.
#[test]
fn the_corner_fixture_is_valid_with_a_leaf_warning() {
    validate("4x4-6u-corner.yaml")
        .success()
        .code(0)
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("compartments: 3"))
        .stdout(predicate::str::contains("15 mm warning threshold"));
}

#[test]
fn every_contract_height_is_valid() {
    for fixture_name in [
        "height-3u.yaml",
        "4x4-6u.yaml",
        "height-9u.yaml",
        "height-12u.yaml",
    ] {
        validate(fixture_name)
            .success()
            .code(0)
            .stderr(predicate::str::is_empty());
    }
}

#[test]
fn every_invalid_spec_exits_two_with_a_precise_message() {
    let cases = [
        ("invalid/system.yaml", "system:"),
        ("invalid/id.yaml", "id:"),
        ("invalid/footprint.yaml", "footprint[0]"),
        ("invalid/height.yaml", "height_u:"),
        ("invalid/lid.yaml", "boolean"),
        ("invalid/unknown-field.yaml", "wall_thickness"),
        ("invalid/unknown-field-division.yaml", "radius"),
        ("invalid/unknown-field-contents.yaml", "print_speed"),
        ("invalid/at-mm.yaml", "at_mm 8 is below the 12 mm minimum"),
        ("invalid/far-child.yaml", "leaf 1.5 mm wide"),
        ("invalid/leaf-small.yaml", "leaf 9.5 mm wide"),
        (
            "invalid/corner-divider.yaml",
            "leaf 1.5 mm deep is below the 12 mm minimum",
        ),
        ("invalid/duplicate.yaml", "duplicate compartment"),
        (
            "invalid/coincident.yaml",
            "coincides with an ancestor divider",
        ),
    ];

    for (fixture_name, needle) in cases {
        validate(fixture_name)
            .failure()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("invalid spec"))
            .stderr(predicate::str::contains(needle));
    }
}

#[test]
fn malformed_yaml_exits_two_with_a_parse_error() {
    validate("invalid/malformed.yaml")
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("YAML parse error"));
}

#[test]
fn a_missing_spec_file_exits_two() {
    let mut command = common::boxgen();
    command
        .arg("validate")
        .arg(common::fixture("does-not-exist.yaml"))
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("cannot read spec"));
}

#[test]
fn warnings_are_reported_without_failing() {
    validate("warning-leaf.yaml")
        .success()
        .code(0)
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("warnings:"))
        .stdout(predicate::str::contains("15 mm warning threshold"));
}

#[test]
fn contents_fit_and_weight_warnings_do_not_fail() {
    validate("warning-contents.yaml")
        .success()
        .code(0)
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("contents.max_length_mm"))
        .stdout(predicate::str::contains("contents.max_width_mm"))
        .stdout(predicate::str::contains("contents.max_height_mm"))
        .stdout(predicate::str::contains("contents.weight_kg"))
        .stdout(predicate::str::contains("10 kg per-body limit"));
}

/// Standard §8: `contents.soft: true` switches the fit check to the soft
/// clearances (5 mm per side, 10 mm above), so an item that fits as rigid
/// warns as soft.
#[test]
fn soft_contents_use_the_soft_clearances() {
    validate("warning-contents-soft.yaml")
        .success()
        .code(0)
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("contents.max_width_mm"))
        .stdout(predicate::str::contains("5 mm per side"))
        .stdout(predicate::str::contains("contents.max_height_mm"))
        .stdout(predicate::str::contains("10 mm clearance"));
}

#[test]
fn mass_and_print_time_warnings_do_not_fail() {
    validate("warning-estimates.yaml")
        .success()
        .code(0)
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("estimated mass"))
        .stdout(predicate::str::contains(
            "exceeds the 550 g warning threshold",
        ))
        .stdout(predicate::str::contains("estimated print time"))
        .stdout(predicate::str::contains(
            "exceeds the 22 h warning threshold",
        ));
}

#[test]
fn all_schema_errors_are_reported_together() {
    validate("invalid/multiple.yaml")
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("4 validation errors"))
        .stderr(predicate::str::contains("system:"))
        .stderr(predicate::str::contains("id:"))
        .stderr(predicate::str::contains("footprint[0]"))
        .stderr(predicate::str::contains("height_u:"));
}
