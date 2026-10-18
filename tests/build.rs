//! `boxgen build` contract: golden script, body and divider artifacts,
//! determinism, and exit codes.
//!
//! The geometry checks need FreeCAD and follow the existing env-gated pattern:
//! they skip on a native host unless `BOXGEN_REQUIRE_FREECAD=1` is set (the
//! Docker gate and CI set it). The golden script and fake launcher tests are
//! pure Rust and always run.
//!
//! Contract: `docs/spec/docs/generator.md` §5 and §6. `freecadcmd` exits 0 on an
//! unhandled Python exception or a missing script, so the build test verifies
//! artifacts rather than trusting the exit status alone.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use predicates::prelude::*;
use serde_json::Value;
use tempfile::tempdir;

const MINIMAL_SPEC: &str = "minimal-build.yaml";

/// The measured 1x1x6U body volume under the pinned FreeCAD (solid base zone,
/// ADR-0013); the test allows ±3% for a geometry-kernel drift.
const EXPECTED_BODY_VOLUME_MM3: f64 = 18_397.5;

/// The measured 2 x 4 x 6U lid volume (the 4 x 4 prototype lid is 54 761.46
/// mm3); ±3% for geometry-kernel drift.
const EXPECTED_LID_VOLUME_MM3: f64 = 26_785.19;

const LID_SPEC: &str = "2x4-6u-lid.yaml";
const LID_RIBS_SPEC: &str = "2x4-6u-lid-ribs.yaml";
const TREE_SPEC: &str = "4x4-6u-tree.yaml";
const CORNER_SPEC: &str = "4x4-6u-corner.yaml";
const EXAMPLE_SPEC: &str = "4x4-6u.yaml";
const WARNING_LEAF_SPEC: &str = "warning-leaf.yaml";

fn build_into(out: &Path) -> Output {
    build_fixture(MINIMAL_SPEC, out)
}

fn build_fixture(name: &str, out: &Path) -> Output {
    common::boxgen()
        .args(["build"])
        .arg(common::fixture(name))
        .arg("--out")
        .arg(out)
        .output()
        .expect("boxgen build runs")
}

fn verify_artifacts(out: &Path) -> Value {
    for name in ["box.py", "box.step", "body.stl", "box.FCStd", "report.json"] {
        let path = out.join(name);
        let metadata = fs::metadata(&path)
            .unwrap_or_else(|error| panic!("missing artifact `{}`: {error}", path.display()));
        assert!(metadata.len() > 0, "`{}` is empty", path.display());
    }

    assert!(
        !out.join("lid.stl").exists(),
        "a lid: false build must not write lid.stl"
    );

    let text = fs::read_to_string(out.join("report.json")).expect("report.json is readable");
    let report: Value = serde_json::from_str(&text).expect("report.json is valid JSON");

    assert_eq!(
        report.pointer("/body/solids").and_then(Value::as_u64),
        Some(1),
        "the body must be exactly one solid: {report}"
    );
    assert!(
        common::number(&report, "body.volume_mm3") > 0.0,
        "the body volume must be positive: {report}"
    );
    assert!(
        common::flag(&report, "body.is_valid"),
        "the body shape must be valid: {report}"
    );
    assert!(
        common::flag(&report, "body.is_closed"),
        "the body must be one closed shell: {report}"
    );
    assert!(
        common::flag(&report, "stl.is_solid"),
        "body.stl must be watertight: {report}"
    );
    assert!(
        report.pointer("/warnings").is_some_and(Value::is_array),
        "report.json warnings must be a list: {report}"
    );

    report
}

/// FreeCAD runs the generated script and the artifacts match the contract.
#[test]
fn minimal_body_builds_end_to_end() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let out = temp.path().join("build/1x1-6U-minimal-v1");
    let output = build_into(&out);
    common::assert_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("id: 1x1-6U-minimal-v1"), "{stdout}");
    assert!(stdout.contains("outer: 41.5 x 41.5 x 42.0 mm"), "{stdout}");
    assert!(stdout.contains("interior depth: 33.25 mm"), "{stdout}");
    assert!(stdout.contains("warnings: none"), "{stdout}");

    let report = verify_artifacts(&out);

    let outer_x = common::number(&report, "dimensions.outer_x_mm");
    let outer_y = common::number(&report, "dimensions.outer_y_mm");
    let height = common::number(&report, "dimensions.height_mm");
    assert!((outer_x - 41.5).abs() <= 0.2, "outer x {outer_x} mm");
    assert!((outer_y - 41.5).abs() <= 0.2, "outer y {outer_y} mm");
    assert!((height - 42.0).abs() <= 0.2, "height {height} mm");

    let interior = common::number(&report, "interior_depth_mm");
    // Standard §4.2: the report carries the depth measured from the model
    // (floor top to lip bottom), not the design formula; 0.05 mm covers the
    // kernel's section tolerance.
    assert!(
        (interior - 33.25).abs() <= 0.05,
        "interior depth {interior} mm"
    );

    let volume = common::number(&report, "body.volume_mm3");
    let tolerance = EXPECTED_BODY_VOLUME_MM3 * 0.03;
    assert!(
        (volume - EXPECTED_BODY_VOLUME_MM3).abs() <= tolerance,
        "body volume {volume} mm3 is outside {} +/- {tolerance} mm3",
        EXPECTED_BODY_VOLUME_MM3
    );

    // Ticket #7: the report records the floor fillet radius actually applied.
    // The 1x1 3U interior takes the standard radius, so the minimal build must
    // never carry the fallback warning.
    let fillet = common::number(&report, "body.floor_fillet_mm");
    assert!(
        (fillet - 2.0).abs() < 1e-9,
        "the floor fillet must be the standard 2.0 mm, report.json says {fillet} mm"
    );
    assert_eq!(
        report
            .pointer("/warnings")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0),
        "a 2.0 mm fillet must not warn: {report}"
    );

    assert!(common::number(&report, "stl.facets") > 0.0);
    assert!(common::number(&report, "stl.volume_mm3") > 0.0);
    assert!(common::number(&report, "mass_g") > 0.0);
    assert!(common::number(&report, "print_time_h") > 0.0);
    assert!(common::number(&report, "artifacts.box_step_bytes") > 0.0);
    assert!(common::number(&report, "artifacts.body_stl_bytes") > 0.0);

    // The build directory is self-contained: the master next to box.py is the
    // only Python code the generated script imports.
    for name in [
        "gridfinity.py",
        "shell.py",
        "build.py",
        "__init__.py",
        "LICENSE-LGPL-2.1",
    ] {
        assert!(
            out.join("cad").join(name).is_file(),
            "missing cad/{name} in the build directory"
        );
    }
    assert!(
        !out.join("cad/tests").exists(),
        "cad/tests must not be copied into a build"
    );

    let generated = fs::read_to_string(out.join("box.py")).expect("box.py is readable");
    assert_eq!(
        generated,
        include_str!("golden/minimal.box.py"),
        "the build wrote a script that differs from the golden snapshot"
    );
}

/// Same spec + same master = same geometry and identical text artifacts.
#[test]
fn two_builds_are_byte_identical_apart_from_the_step_timestamp() {
    if !common::freecad_required() {
        return;
    }

    let first = tempdir().expect("first tempdir");
    let second = tempdir().expect("second tempdir");
    let out_a = first.path().join("out");
    let out_b = second.path().join("out");

    common::assert_success(&build_into(&out_a));
    common::assert_success(&build_into(&out_b));

    for name in ["box.py", "body.stl", "report.json"] {
        let a = fs::read(out_a.join(name)).expect("first artifact");
        let b = fs::read(out_b.join(name)).expect("second artifact");
        assert_eq!(a, b, "`{name}` differs between two identical builds");
    }

    let step_a = fs::read_to_string(out_a.join("box.step")).expect("first STEP");
    let step_b = fs::read_to_string(out_b.join("box.step")).expect("second STEP");
    assert_eq!(
        common::normalise_step(&step_a),
        common::normalise_step(&step_b),
        "box.step differs beyond the FILE_NAME timestamp"
    );
}

/// `freecadcmd` exits 0 even on an unhandled exception; the launcher itself
/// must still exist and report the pinned release.
#[test]
fn missing_freecadcmd_is_an_environment_error() {
    let temp = tempdir().expect("tempdir");
    let out = temp.path().join("out");

    common::boxgen()
        .args(["build"])
        .arg(common::fixture(MINIMAL_SPEC))
        .arg("--out")
        .arg(&out)
        .env("BOXGEN_FREECADCMD", "/nonexistent/freecadcmd")
        .assert()
        .code(4)
        .stderr(
            predicate::str::contains("BOXGEN_FREECADCMD")
                .and(predicate::str::contains("FreeCAD 1.0.0")),
        );
}

#[test]
fn the_wrong_freecad_version_is_an_environment_error() {
    let temp = tempdir().expect("tempdir");
    let launcher = common::fake_launcher(temp.path(), "echo 'FreeCAD 0.19.0 Revision:'\nexit 0");

    common::boxgen()
        .args(["build"])
        .arg(common::fixture(MINIMAL_SPEC))
        .arg("--out")
        .arg(temp.path().join("out"))
        .env("BOXGEN_FREECADCMD", &launcher)
        .assert()
        .code(4)
        .stderr(predicate::str::contains("0.19.0").and(predicate::str::contains("1.0.0")));
}

/// A launcher that passes the version check but fails to build is exit 3.
#[test]
fn a_freecad_failure_is_a_build_error() {
    let temp = tempdir().expect("tempdir");
    let launcher = common::fake_launcher(
        temp.path(),
        "if [ \"$1\" = \"--version\" ]; then echo 'FreeCAD 1.0.0 Revision:'; exit 0; fi\n\
         echo 'simulated geometry failure' >&2\n\
         exit 1",
    );

    common::boxgen()
        .args(["build"])
        .arg(common::fixture(MINIMAL_SPEC))
        .arg("--out")
        .arg(temp.path().join("out"))
        .env("BOXGEN_FREECADCMD", &launcher)
        .assert()
        .code(3)
        .stderr(
            predicate::str::contains("freecadcmd exited with")
                .and(predicate::str::contains("simulated geometry failure")),
        );
}

/// A launcher that exits 0 without writing artifacts is still a build error.
#[test]
fn missing_artifacts_are_a_build_error() {
    let temp = tempdir().expect("tempdir");
    let launcher = common::fake_launcher(
        temp.path(),
        "if [ \"$1\" = \"--version\" ]; then echo 'FreeCAD 1.0.0 Revision:'; exit 0; fi\n\
         exit 0",
    );

    common::boxgen()
        .args(["build"])
        .arg(common::fixture(MINIMAL_SPEC))
        .arg("--out")
        .arg(temp.path().join("out"))
        .env("BOXGEN_FREECADCMD", &launcher)
        .assert()
        .code(3)
        .stderr(
            predicate::str::contains("report.json").and(predicate::str::contains("did not write")),
        );
}

#[test]
fn empty_artifacts_are_a_build_error() {
    let temp = tempdir().expect("tempdir");
    let launcher = common::fake_launcher(
        temp.path(),
        r#"if [ "$1" = "--version" ]; then echo 'FreeCAD 1.0.0 Revision:'; exit 0; fi
out=$(dirname "$1")
printf 'x' > "$out/box.step"
: > "$out/body.stl"
printf 'x' > "$out/box.FCStd"
printf '{}' > "$out/report.json"
exit 0"#,
    );

    common::boxgen()
        .args(["build"])
        .arg(common::fixture(MINIMAL_SPEC))
        .arg("--out")
        .arg(temp.path().join("out"))
        .env("BOXGEN_FREECADCMD", &launcher)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("empty"));
}

#[test]
fn unparseable_report_json_is_a_build_error() {
    let temp = tempdir().expect("tempdir");
    let launcher = common::fake_launcher(
        temp.path(),
        r#"if [ "$1" = "--version" ]; then echo 'FreeCAD 1.0.0 Revision:'; exit 0; fi
out=$(dirname "$1")
printf 'x' > "$out/box.step"
printf 'x' > "$out/body.stl"
printf 'x' > "$out/box.FCStd"
printf '{' > "$out/report.json"
exit 0"#,
    );

    common::boxgen()
        .args(["build"])
        .arg(common::fixture(MINIMAL_SPEC))
        .arg("--out")
        .arg(temp.path().join("out"))
        .env("BOXGEN_FREECADCMD", &launcher)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("not valid JSON"));
}

/// Validation warnings are advisory; they must reach report.json and stdout
/// without changing the exit code.
#[test]
fn validation_warnings_are_added_to_the_report() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let spec = "\
system: SYS-1.0
id: 1x1-6U-warning-v1
footprint: [1, 1]
height_u: 6
lid: false
contents:
  max_width_mm: 40
divisions: { compartment: \"Bits\" }
";
    let path = temp.path().join("warning.yaml");
    fs::write(&path, spec).expect("write spec");
    let out = temp.path().join("out");

    let output = common::boxgen()
        .args(["build"])
        .arg(&path)
        .arg("--out")
        .arg(&out)
        .output()
        .expect("boxgen build runs");
    common::assert_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("warnings:"), "{stdout}");
    assert!(stdout.contains("contents.max_width_mm"), "{stdout}");

    let report = verify_artifacts(&out);
    let warnings = report["warnings"].as_array().expect("warnings list");
    assert!(
        !warnings.is_empty(),
        "warnings must be carried into report.json"
    );
    assert!(
        warnings.iter().any(|warning| warning
            .as_str()
            .is_some_and(|warning| warning.contains("contents.max_width_mm"))),
        "missing contents warning: {warnings:?}"
    );
}

#[test]
fn an_invalid_spec_fails_with_exit_two_before_freecad() {
    let temp = tempdir().expect("tempdir");

    common::boxgen()
        .args(["build"])
        .arg(common::fixture("invalid/leaf-small.yaml"))
        .arg("--out")
        .arg(temp.path().join("out"))
        .env("BOXGEN_FREECADCMD", "/nonexistent/freecadcmd")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("validation error"));
}

/// A lid spec runs through the ordinary build path: `lid.stl` and the
/// report's lid section are both required, and a complete lid build succeeds.
#[test]
fn a_lid_build_requires_lid_stl_and_the_lid_report_section() {
    let temp = tempdir().expect("tempdir");
    let spec = fs::read_to_string(common::fixture(MINIMAL_SPEC))
        .expect("minimal fixture")
        .replace("lid: false", "lid: true");
    let path = temp.path().join("with-lid.yaml");
    fs::write(&path, spec).expect("write spec");

    // Missing lid.stl: the build fails and names the file.
    let launcher =
        launcher_with_artifacts(temp.path(), &fake_report("1x1-6U-minimal-v1", true), None);
    common::boxgen()
        .args(["build"])
        .arg(&path)
        .arg("--out")
        .arg(temp.path().join("out"))
        .env("BOXGEN_FREECADCMD", &launcher)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("lid.stl"));

    // lid.stl present but the report has no lid section: still a failure.
    let launcher = launcher_with_artifacts(
        temp.path(),
        &fake_report("1x1-6U-minimal-v1", false),
        Some("LID"),
    );
    common::boxgen()
        .args(["build"])
        .arg(&path)
        .arg("--out")
        .arg(temp.path().join("out"))
        .env("BOXGEN_FREECADCMD", &launcher)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("lid section"));

    // Complete: the build succeeds, lid.stl is an artifact, and the summary
    // names the lid.
    let launcher = launcher_with_artifacts(
        temp.path(),
        &fake_report("1x1-6U-minimal-v1", true),
        Some("LID"),
    );
    let output = common::boxgen()
        .args(["build"])
        .arg(&path)
        .arg("--out")
        .arg(temp.path().join("out"))
        .env("BOXGEN_FREECADCMD", &launcher)
        .output()
        .expect("boxgen build runs");
    common::assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("lid: "), "{stdout}");
    assert!(stdout.contains("lid.stl"), "{stdout}");
    assert!(
        temp.path().join("out/lid.stl").is_file(),
        "a lid build must write lid.stl"
    );
}

/// A body-only spec must stay body-only: no lid.stl, even when a stale one
/// from an earlier lid build is still in the output directory.
#[test]
fn a_body_only_build_removes_a_stale_lid_stl() {
    let temp = tempdir().expect("tempdir");
    let out = temp.path().join("out");
    fs::create_dir_all(&out).expect("out dir");
    fs::write(out.join("lid.stl"), "STALE").expect("stale lid");

    let launcher =
        launcher_with_artifacts(temp.path(), &fake_report("1x1-6U-minimal-v1", false), None);
    common::boxgen()
        .args(["build"])
        .arg(common::fixture(MINIMAL_SPEC))
        .arg("--out")
        .arg(&out)
        .env("BOXGEN_FREECADCMD", &launcher)
        .assert()
        .success();

    assert!(
        !out.join("lid.stl").exists(),
        "a body-only build must clear a stale lid.stl"
    );
}

/// The contract's golden 4 x 4 tree (x at 80; left child y at 50; right child
/// x at 40) builds end to end: exit 0, the full artifact set, one valid closed
/// body, and the resolved divider plan in `report.json`.
#[test]
fn a_split_tree_builds_end_to_end() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let out = temp.path().join("build/4x4-6U-tree-v1");
    common::assert_success(&build_fixture(TREE_SPEC, &out));

    let report = verify_artifacts(&out);

    assert_eq!(
        common::number(&report, "body.solids"),
        1.0,
        "the body with dividers must be one solid: {report}"
    );
    assert_eq!(common::number(&report, "dividers.count"), 3.0);
    assert_eq!(
        common::number(&report, "dividers.height_above_floor_mm"),
        23.25
    );
    assert_eq!(common::number(&report, "dividers.top_mm"), 28.0);

    let plan = report
        .pointer("/dividers/plan")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("report.json has no divider plan: {report}"));
    assert_eq!(plan.len(), 3, "one plan entry per split: {plan:?}");

    let find = |axis: &str, near_face: f64| -> &Value {
        plan.iter()
            .find(|divider| {
                divider["axis"].as_str() == Some(axis)
                    && (divider["near_face_mm"].as_f64().expect("near face") - near_face).abs()
                        < 1e-6
            })
            .unwrap_or_else(|| panic!("no {axis} divider at {near_face} in {plan:?}"))
    };

    let root = find("x", -1.75);
    assert_eq!(root["far_face_mm"].as_f64(), Some(0.25), "{root}");
    assert_eq!(root["span_lo_mm"].as_f64(), Some(-81.75), "{root}");
    assert_eq!(root["span_hi_mm"].as_f64(), Some(81.75), "{root}");

    let left = find("y", -31.75);
    assert_eq!(left["span_lo_mm"].as_f64(), Some(-81.75), "{left}");
    assert_eq!(left["span_hi_mm"].as_f64(), Some(-1.75), "{left}");
    assert_eq!(left["hi_end"].as_str(), Some("tee"), "{left}");

    let right = find("x", 40.25);
    assert_eq!(right["span_lo_mm"].as_f64(), Some(-81.75), "{right}");
    assert_eq!(right["span_hi_mm"].as_f64(), Some(81.75), "{right}");
}

/// Asserts that a slice's wire boxes match `expected`, in any order.
fn assert_slice_wires(slices: &Value, key: &str, expected: &[[f64; 4]]) {
    let wires = slices[key]
        .as_array()
        .unwrap_or_else(|| panic!("slice report has no {key}: {slices}"));
    assert_eq!(
        wires.len(),
        expected.len(),
        "{key}: expected {} wires, measured {}: {wires:?}",
        expected.len(),
        wires.len()
    );

    let mut unmatched: Vec<[f64; 4]> = expected.to_vec();
    for wire in wires {
        let actual = [
            wire[0].as_f64().expect("x_min"),
            wire[1].as_f64().expect("y_min"),
            wire[2].as_f64().expect("x_max"),
            wire[3].as_f64().expect("y_max"),
        ];
        let Some(position) = unmatched.iter().position(|want| {
            want.iter()
                .zip(actual.iter())
                .all(|(want, got)| (want - got).abs() < 1e-6)
        }) else {
            panic!("{key}: wire {actual:?} matches no expected box in {expected:?}");
        };
        unmatched.remove(position);
    }
}

/// The exported model carries the dividers: slicing the STEP at z = 20 (well
/// below the 28.0 top) shows exactly the four contract compartments, which
/// pin the three near faces at x = -1.75, y = -31.75, x = 40.25 and their
/// full spans; z = 30 shows one open interior.
#[test]
fn the_exported_golden_tree_step_slices_show_the_three_dividers() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let out = temp.path().join("build/4x4-6U-tree-v1");
    common::assert_success(&build_fixture(TREE_SPEC, &out));

    let slices = common::slice_step(&out.join("box.step"), "20,30");
    assert_eq!(
        slices["objects"].as_array().map(Vec::len),
        Some(1),
        "a body-only STEP carries only the Body object: {slices}"
    );
    assert_eq!(slices["objects"][0].as_str(), Some("Body"));
    assert_eq!(slices["solids"].as_u64(), Some(1));
    assert_eq!(slices["is_closed"].as_bool(), Some(true));

    assert_slice_wires(
        &slices,
        "section_z20",
        &[
            [-83.75, -83.75, 83.75, 83.75],
            [-81.75, -81.75, -1.75, -31.75],
            [-81.75, -29.75, -1.75, 81.75],
            [0.25, -81.75, 40.25, 81.75],
            [42.25, -81.75, 81.75, 81.75],
        ],
    );
    assert_slice_wires(
        &slices,
        "section_z30",
        &[
            [-83.75, -83.75, 83.75, 83.75],
            [-81.75, -81.75, 81.75, 81.75],
        ],
    );
}

/// A split tree with a leaf below the 15 mm warning threshold still builds
/// (exit 0) and the validation warning reaches report.json and stdout.
#[test]
fn a_small_leaf_warning_flows_through_a_split_build() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let out = temp.path().join("out");
    let output = build_fixture(WARNING_LEAF_SPEC, &out);
    common::assert_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("warnings:"), "{stdout}");

    let report = verify_artifacts(&out);
    let warnings = report["warnings"].as_array().expect("warnings list");
    assert!(
        warnings.iter().any(|warning| warning
            .as_str()
            .is_some_and(|warning| warning.contains("15 mm warning threshold"))),
        "the small-leaf warning must reach report.json: {warnings:?}"
    );
    assert_eq!(common::number(&report, "dividers.count"), 1.0);
}

/// The deepest nesting contract section 4 admits: the leaf against the +x/+y
/// corner is 12.5 mm, so the build succeeds with the small-leaf warning and
/// the exported model carries the two nested dividers.
#[test]
fn the_deepest_legal_corner_nesting_builds() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let out = temp.path().join("build/4x4-6U-corner-v1");
    common::assert_success(&build_fixture(CORNER_SPEC, &out));

    let report = verify_artifacts(&out);
    assert_eq!(common::number(&report, "dividers.count"), 2.0);
    let warnings = report["warnings"].as_array().expect("warnings list");
    assert!(
        warnings.iter().any(|warning| warning
            .as_str()
            .is_some_and(|warning| warning.contains("15 mm warning threshold"))),
        "the 12.5 mm corner leaf must warn: {warnings:?}"
    );

    let slices = common::slice_step(&out.join("box.step"), "20");
    assert_slice_wires(
        &slices,
        "section_z20",
        &[
            [-83.75, -83.75, 83.75, 83.75],
            [-81.75, -81.75, 67.25, 67.25],
            [-81.75, 69.25, 67.25, 81.75],
            [69.25, -81.75, 81.75, 81.75],
        ],
    );
}

/// Body + dividers + lid stay one document: the example 4 x 4 fixture has
/// three splits and `lid: true`, and both print files come out.
#[test]
fn a_lid_build_with_dividers_assembles_both_solids() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let out = temp.path().join("build/4x4-6U-example-v1");
    common::assert_success(&build_fixture(EXAMPLE_SPEC, &out));

    for name in [
        "box.step",
        "body.stl",
        "lid.stl",
        "box.FCStd",
        "report.json",
    ] {
        let path = out.join(name);
        let metadata = fs::metadata(&path)
            .unwrap_or_else(|error| panic!("missing artifact `{}`: {error}", path.display()));
        assert!(metadata.len() > 0, "`{}` is empty", path.display());
    }

    let step = fs::read_to_string(out.join("box.step")).expect("box.step");
    assert!(step.contains("PRODUCT('Body'"), "STEP has no Body product");
    assert!(step.contains("PRODUCT('Lid'"), "STEP has no Lid product");

    let text = fs::read_to_string(out.join("report.json")).expect("report.json");
    let report: Value = serde_json::from_str(&text).expect("report.json is valid JSON");
    assert_eq!(common::number(&report, "dividers.count"), 3.0);
    assert_eq!(common::number(&report, "lid.solids"), 1.0);
}

/// Same spec + same master = same geometry for a split tree too.
#[test]
fn two_tree_builds_are_byte_identical_apart_from_the_step_timestamp() {
    if !common::freecad_required() {
        return;
    }

    let first = tempdir().expect("first tempdir");
    let second = tempdir().expect("second tempdir");
    let out_a = first.path().join("out");
    let out_b = second.path().join("out");

    common::assert_success(&build_fixture(TREE_SPEC, &out_a));
    common::assert_success(&build_fixture(TREE_SPEC, &out_b));

    for name in ["box.py", "body.stl", "report.json"] {
        let a = fs::read(out_a.join(name)).expect("first artifact");
        let b = fs::read(out_b.join(name)).expect("second artifact");
        assert_eq!(a, b, "`{name}` differs between two identical builds");
    }

    let step_a = fs::read_to_string(out_a.join("box.step")).expect("first STEP");
    let step_b = fs::read_to_string(out_b.join("box.step")).expect("second STEP");
    assert_eq!(
        common::normalise_step(&step_a),
        common::normalise_step(&step_b),
        "box.step differs beyond the FILE_NAME timestamp"
    );
}

/// The publish target is the spec's own box folder: a spec elsewhere fails
/// with a usage error before FreeCAD runs, and the folder stays untouched.
#[test]
fn publish_outside_the_boxes_id_layout_fails_clearly() {
    let temp = tempdir().expect("tempdir");
    let folder = temp.path().join("not-a-box");
    fs::create_dir_all(&folder).expect("folder");
    let spec = folder.join("spec.yaml");
    fs::write(
        &spec,
        fs::read_to_string(common::fixture(MINIMAL_SPEC)).expect("fixture"),
    )
    .expect("spec");

    common::boxgen()
        .args(["build"])
        .arg(&spec)
        .arg("--publish")
        .env("BOXGEN_FREECADCMD", "/nonexistent/freecadcmd")
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("boxes/<id>/spec.yaml")
                .and(predicate::str::contains("1x1-6U-minimal-v1")),
        );

    assert_eq!(
        fs::read_dir(&folder).expect("folder listing").count(),
        1,
        "the mismatch must not touch the folder"
    );
}

/// A missing body after a successful-looking launcher is a build error, and
/// publish never runs.
#[test]
fn a_missing_body_stl_fails_before_publish_and_leaves_the_box_folder_untouched() {
    let temp = tempdir().expect("tempdir");
    let folder = box_folder(temp.path(), "1x1-6U-minimal-v1");
    let launcher = common::fake_launcher(
        temp.path(),
        r#"if [ "$1" = "--version" ]; then echo 'FreeCAD 1.0.0 Revision:'; exit 0; fi
out=$(dirname "$1")
printf 'x' > "$out/box.step"
printf 'x' > "$out/box.FCStd"
printf '{}' > "$out/report.json"
exit 0"#,
    );

    common::boxgen()
        .args(["build", "boxes/1x1-6U-minimal-v1/spec.yaml", "--publish"])
        .current_dir(temp.path())
        .env("BOXGEN_FREECADCMD", &launcher)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("body.stl"));

    assert_eq!(
        folder_names(&folder),
        ["spec.yaml"],
        "a failed build must not publish anything"
    );
}

/// A successful publish copies the STEP, the body STL, and the generated
/// notes into the box folder and nothing else.
#[test]
fn build_publish_populates_the_box_folder_with_the_contract_artifacts() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let folder = box_folder(temp.path(), "1x1-6U-minimal-v1");
    let spec_before = fs::read(folder.join("spec.yaml")).expect("spec bytes");

    let output = common::boxgen()
        .args(["build", "boxes/1x1-6U-minimal-v1/spec.yaml", "--publish"])
        .current_dir(temp.path())
        .output()
        .expect("boxgen build --publish");
    common::assert_success(&output);

    for name in ["box.step", "body.stl", "notes.md"] {
        let path = folder.join(name);
        let metadata = fs::metadata(&path)
            .unwrap_or_else(|error| panic!("missing published `{}`: {error}", path.display()));
        assert!(metadata.len() > 0, "`{}` is empty", path.display());
    }
    for name in ["lid.stl", "box.FCStd", "report.json", "box.py", "cad"] {
        assert!(
            !folder.join(name).exists(),
            "`{name}` must never be published"
        );
    }
    assert_eq!(
        folder_names(&folder),
        ["body.stl", "box.step", "notes.md", "spec.yaml"]
    );
    assert_eq!(
        fs::read(folder.join("spec.yaml")).expect("spec bytes"),
        spec_before,
        "publish must never rewrite spec.yaml"
    );

    let notes = fs::read_to_string(folder.join("notes.md")).expect("notes");
    assert!(notes.starts_with("# 1x1-6U-minimal-v1\n"), "{notes}");
    assert!(notes.contains("Stack group: F"), "{notes}");
    assert!(notes.contains("\nPrinted:\n"), "{notes}");

    assert!(
        temp.path()
            .join("build/1x1-6U-minimal-v1/box.step")
            .is_file(),
        "the build directory keeps its artifacts"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let summary = stdout.find("artifacts:").expect("the summary block");
    let published = stdout
        .find("published: boxes/1x1-6U-minimal-v1\n")
        .expect("the published line");
    assert!(
        summary < published,
        "the published line follows the summary: {stdout}"
    );
}

/// `notes.md` is the human print record: publish never overwrites an existing
/// one, even when the build has its own generated copy.
#[test]
fn publish_never_overwrites_a_handwritten_notes_file() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let folder = box_folder(temp.path(), "1x1-6U-minimal-v1");
    let record = "Printed: 2026-10-01\nFilament: PETG Basic lot 7\n";
    fs::write(folder.join("notes.md"), record).expect("handwritten notes");

    let output = common::boxgen()
        .args(["build", "boxes/1x1-6U-minimal-v1/spec.yaml", "--publish"])
        .current_dir(temp.path())
        .output()
        .expect("boxgen build --publish");
    common::assert_success(&output);

    assert_eq!(
        fs::read_to_string(folder.join("notes.md")).expect("notes"),
        record
    );
    assert!(folder.join("box.step").is_file());
}

/// Without `--publish` the box folder and the registry index stay untouched.
#[test]
fn build_without_publish_leaves_boxes_untouched() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let folder = box_folder(temp.path(), "1x1-6U-minimal-v1");

    let output = common::boxgen()
        .args(["build", "boxes/1x1-6U-minimal-v1/spec.yaml"])
        .current_dir(temp.path())
        .output()
        .expect("boxgen build");
    common::assert_success(&output);

    assert_eq!(folder_names(&folder), ["spec.yaml"]);
    assert!(!temp.path().join("boxes/INDEX.md").exists());
    assert!(
        temp.path()
            .join("build/1x1-6U-minimal-v1/body.stl")
            .is_file()
    );
}

/// A body-only STEP carries exactly one `Part::Feature`: no lid object.
#[test]
fn a_body_only_step_has_no_lid_object() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let out = temp.path().join("build/1x1-6U-minimal-v1");
    common::assert_success(&build_into(&out));

    let step = fs::read_to_string(out.join("box.step")).expect("box.step is readable");
    assert!(step.contains("PRODUCT('Body'"), "{step}");
    assert!(
        !step.contains("PRODUCT('Lid'"),
        "a body-only STEP must not contain a Lid object"
    );
}

/// A lid spec writes both print files, one STEP with both solids, and the
/// report's lid measurements. The geometry itself is pinned by `tests/lid.rs`;
/// this checks the end-to-end build contract.
#[test]
fn a_lid_build_writes_both_print_files_and_measures_the_lid() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let out = temp.path().join("build/2x4-6U-lid-box-v1");
    common::assert_success(&build_fixture(LID_SPEC, &out));

    for name in [
        "box.py",
        "box.step",
        "body.stl",
        "lid.stl",
        "box.FCStd",
        "report.json",
    ] {
        let path = out.join(name);
        let metadata = fs::metadata(&path)
            .unwrap_or_else(|error| panic!("missing artifact `{}`: {error}", path.display()));
        assert!(metadata.len() > 0, "`{}` is empty", path.display());
    }

    let text = fs::read_to_string(out.join("report.json")).expect("report.json is readable");
    let report: Value = serde_json::from_str(&text).expect("report.json is valid JSON");

    assert_eq!(
        report.pointer("/lid/solids").and_then(Value::as_u64),
        Some(1),
        "the lid must be exactly one solid: {report}"
    );
    assert!(
        common::flag(&report, "lid.is_valid"),
        "the lid shape must be valid: {report}"
    );
    assert!(
        common::flag(&report, "lid.is_closed"),
        "the lid must be one closed shell: {report}"
    );
    assert!(
        !common::flag(&report, "lid.ribs"),
        "the plain lid fixture has no friction ribs: {report}"
    );
    assert_eq!(
        common::number(&report, "lid.roundover_mm"),
        1.0,
        "the plate top roundover must be applied: {report}"
    );
    assert!(
        common::flag(&report, "lid_stl.is_solid"),
        "lid.stl must be watertight: {report}"
    );
    assert!(
        common::number(&report, "artifacts.lid_stl_bytes") > 0.0,
        "the report must record the lid.stl size: {report}"
    );

    let volume = common::number(&report, "lid.volume_mm3");
    let tolerance = EXPECTED_LID_VOLUME_MM3 * 0.03;
    assert!(
        (volume - EXPECTED_LID_VOLUME_MM3).abs() <= tolerance,
        "lid volume {volume} mm3 is outside {EXPECTED_LID_VOLUME_MM3} +/- {tolerance} mm3"
    );

    // Standard section 4.4: plate top 1.0 mm below the 42.0 mm lip top, skirt
    // 1.0 mm below the 38.0 mm lip bottom; plate 0.30 mm clear per side.
    assert!((common::number(&report, "lid.bbox.z_max") - 41.0).abs() < 1e-6);
    assert!((common::number(&report, "lid.bbox.z_min") - 37.0).abs() < 1e-6);
    assert!(
        (common::number(&report, "lid.bbox.x_max")
            - common::number(&report, "lid.bbox.x_min")
            - 79.1)
            .abs()
            < 1e-6
    );
    assert!(
        (common::number(&report, "lid.bbox.y_max")
            - common::number(&report, "lid.bbox.y_min")
            - 163.1)
            .abs()
            < 1e-6
    );

    let step = fs::read_to_string(out.join("box.step")).expect("box.step is readable");
    assert!(step.contains("PRODUCT('Body'"), "STEP has no Body product");
    assert!(step.contains("PRODUCT('Lid'"), "STEP has no Lid product");
}

/// `lid_ribs: true` adds the standard's optional ribs; the report records the
/// switch and the ribbed lid's solid volume is larger than the plain one.
#[test]
fn the_ribbed_lid_build_reports_the_optional_ribs() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let plain_out = temp.path().join("plain");
    let ribs_out = temp.path().join("ribs");
    common::assert_success(&build_fixture(LID_SPEC, &plain_out));
    common::assert_success(&build_fixture(LID_RIBS_SPEC, &ribs_out));

    let plain: Value = serde_json::from_str(
        &fs::read_to_string(plain_out.join("report.json")).expect("plain report"),
    )
    .expect("plain report is JSON");
    let ribs: Value = serde_json::from_str(
        &fs::read_to_string(ribs_out.join("report.json")).expect("ribs report"),
    )
    .expect("ribs report is JSON");

    assert!(
        common::flag(&ribs, "lid.ribs"),
        "the ribbed fixture must report ribs: {ribs}"
    );
    assert!(
        !common::flag(&plain, "lid.ribs"),
        "the plain fixture must not report ribs: {plain}"
    );
    assert!(
        ribs_out.join("lid.stl").is_file(),
        "the ribbed build must write lid.stl"
    );
    assert!(
        common::number(&ribs, "lid.volume_mm3") > common::number(&plain, "lid.volume_mm3"),
        "the ribs must add material"
    );
}

/// `lid_ribs: true` with `lid: false` is an invalid spec and exits 2 before
/// FreeCAD starts, naming both fields.
#[test]
fn lid_ribs_without_a_lid_exits_two() {
    let temp = tempdir().expect("tempdir");
    let spec = fs::read_to_string(common::fixture(MINIMAL_SPEC))
        .expect("minimal fixture")
        .replace("lid: false", "lid: false\nlid_ribs: true");
    let path = temp.path().join("ribs-without-lid.yaml");
    fs::write(&path, spec).expect("write spec");

    common::boxgen()
        .args(["build"])
        .arg(&path)
        .arg("--out")
        .arg(temp.path().join("out"))
        .env("BOXGEN_FREECADCMD", "/nonexistent/freecadcmd")
        .assert()
        .code(2)
        .stderr(
            predicate::str::contains("validation error")
                .and(predicate::str::contains("lid_ribs"))
                .and(predicate::str::contains("lid: true")),
        );
}

/// Same spec + same master = same geometry and identical text artifacts,
/// including the lid print file and the report's lid section.
#[test]
fn two_lid_builds_are_byte_identical_apart_from_the_step_timestamp() {
    if !common::freecad_required() {
        return;
    }

    let first = tempdir().expect("first tempdir");
    let second = tempdir().expect("second tempdir");
    let out_a = first.path().join("out");
    let out_b = second.path().join("out");

    common::assert_success(&build_fixture(LID_SPEC, &out_a));
    common::assert_success(&build_fixture(LID_SPEC, &out_b));

    for name in ["box.py", "body.stl", "lid.stl", "report.json"] {
        let a = fs::read(out_a.join(name)).expect("first artifact");
        let b = fs::read(out_b.join(name)).expect("second artifact");
        assert_eq!(a, b, "`{name}` differs between two identical lid builds");
    }

    let step_a = fs::read_to_string(out_a.join("box.step")).expect("first STEP");
    let step_b = fs::read_to_string(out_b.join("box.step")).expect("second STEP");
    assert_eq!(
        common::normalise_step(&step_a),
        common::normalise_step(&step_b),
        "the lid STEP differs beyond the FILE_NAME timestamp"
    );
}

/// `--publish` copies `lid.stl` alongside the body print file for a lid spec.
#[test]
fn build_publish_copies_lid_stl_for_a_lid_spec() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let id = "2x4-6U-lid-box-v1";
    let folder = box_folder(temp.path(), id);
    fs::write(
        folder.join("spec.yaml"),
        fs::read_to_string(common::fixture(LID_SPEC)).expect("lid fixture"),
    )
    .expect("write lid spec");

    let output = common::boxgen()
        .args(["build", &format!("boxes/{id}/spec.yaml"), "--publish"])
        .current_dir(temp.path())
        .output()
        .expect("boxgen build --publish");
    common::assert_success(&output);

    assert_eq!(
        folder_names(&folder),
        ["body.stl", "box.step", "lid.stl", "notes.md", "spec.yaml"],
        "publish must copy both print files and nothing else"
    );
    for name in ["box.step", "body.stl", "lid.stl", "notes.md"] {
        let path = folder.join(name);
        let metadata = fs::metadata(&path)
            .unwrap_or_else(|error| panic!("missing published `{}`: {error}", path.display()));
        assert!(metadata.len() > 0, "`{}` is empty", path.display());
    }
}

/// Creates `boxes/<id>/spec.yaml` from the minimal fixture and returns the
/// box folder.
fn box_folder(root: &Path, id: &str) -> PathBuf {
    let folder = root.join("boxes").join(id);
    fs::create_dir_all(&folder).expect("box folder");
    fs::write(
        folder.join("spec.yaml"),
        fs::read_to_string(common::fixture(MINIMAL_SPEC)).expect("fixture"),
    )
    .expect("write spec");
    folder
}

fn folder_names(folder: &Path) -> Vec<String> {
    let mut names = fs::read_dir(folder)
        .expect("read box folder")
        .map(|entry| {
            entry
                .expect("box folder entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// A complete, contract-shaped fake `report.json` for a 1x1x6U spec, with or
/// without the lid sections.
fn fake_report(id: &str, lid: bool) -> String {
    let mut value = serde_json::json!({
        "id": id,
        "dimensions": {"outer_x_mm": 41.5, "outer_y_mm": 41.5, "height_mm": 42.0},
        "interior_depth_mm": 33.25,
        "body": {
            "volume_mm3": 18397.5,
            "area_mm2": 1.0,
            "faces": 1,
            "solids": 1,
            "is_valid": true,
            "is_closed": true,
            "floor_fillet_mm": 2.0,
        },
        "stl": {"facets": 1, "is_solid": true, "volume_mm3": 1.0},
        "artifacts": {"box_step_bytes": 1, "body_stl_bytes": 1},
        "mass_g": 23.4,
        "print_time_h": 0.93,
        "warnings": [],
    });
    if lid {
        value["lid"] = serde_json::json!({
            "volume_mm3": 2584.66,
            "area_mm2": 1.0,
            "faces": 1,
            "solids": 1,
            "is_valid": true,
            "is_closed": true,
            "roundover_mm": 1.0,
            "ribs": false,
            "bbox": {
                "x_min": -18.55, "x_max": 18.55,
                "y_min": -18.55, "y_max": 18.55,
                "z_min": 37.0, "z_max": 41.0,
            },
        });
        value["lid_stl"] = serde_json::json!({"facets": 1, "is_solid": true, "volume_mm3": 1.0});
        value["artifacts"]["lid_stl_bytes"] = serde_json::json!(1);
    }
    serde_json::to_string(&value).expect("a JSON value serializes")
}

/// A launcher that passes the version check and writes every artifact,
/// copying the given report into `report.json`. When `lid_stl` is `None` the
/// lid print file is omitted, simulating an incomplete lid build.
fn launcher_with_artifacts(dir: &Path, report: &str, lid_stl: Option<&str>) -> PathBuf {
    let report_path = dir.join("report-source.json");
    fs::write(&report_path, report).expect("write report source");
    let lid_line = match lid_stl {
        Some(marker) => format!("printf '{marker}' > \"$out/lid.stl\"\n"),
        None => String::new(),
    };
    common::fake_launcher(
        dir,
        &format!(
            "if [ \"$1\" = \"--version\" ]; then echo 'FreeCAD 1.0.0 Revision:'; exit 0; fi\n\
             out=$(dirname \"$1\")\n\
             printf 'STEP' > \"$out/box.step\"\n\
             printf 'BODY' > \"$out/body.stl\"\n\
             printf 'FCSTD' > \"$out/box.FCStd\"\n\
             {lid_line}\
             cp '{report}' \"$out/report.json\"\n\
             exit 0",
            report = report_path.display()
        ),
    )
}

#[test]
fn the_step_normaliser_ignores_the_file_name_timestamp() {
    let first = "FILE_NAME('Open CASCADE Shape Model','2026-10-17T16:17:24',('Author'),(\n    ''),'FreeCAD');";
    let second = "FILE_NAME('Open CASCADE Shape Model','2026-10-17T16:17:33',('Author'),(\n    ''),'FreeCAD');";

    assert_eq!(
        common::normalise_step(first),
        common::normalise_step(second)
    );
    assert!(common::normalise_step(first).contains("<timestamp>"));
}

#[test]
fn the_golden_script_is_a_pure_function_of_the_spec() {
    let spec = boxgen::spec::load(&common::fixture(MINIMAL_SPEC)).expect("fixture parses");
    let rendered = boxgen::build::render_box_py(&spec);

    assert_eq!(
        rendered,
        include_str!("golden/minimal.box.py"),
        "generated box.py differs from the golden snapshot; update \
         tests/golden/minimal.box.py only for an intentional change"
    );
}

/// The lid fixture pins the `lid: true` script, including the `lid_ribs`
/// switch in the embedded spec.
#[test]
fn the_lid_golden_script_is_a_pure_function_of_the_spec() {
    let spec = boxgen::spec::load(&common::fixture("2x4-6u-lid.yaml")).expect("fixture parses");
    let rendered = boxgen::build::render_box_py(&spec);

    assert_eq!(
        rendered,
        include_str!("golden/2x4-6u-lid.box.py"),
        "generated box.py differs from the golden snapshot; update \
         tests/golden/2x4-6u-lid.box.py only for an intentional change"
    );
}
