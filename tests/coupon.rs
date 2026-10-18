//! `boxgen coupon` contract (ticket #10, calibration §2).
//!
//! The coupon is the one internal 1 x 1 x 6U build: the standard shell with
//! the calibration T-divider, the ribbed friction lid, and the internal
//! `coupon: true` flag. It runs through the same build pipeline as `build`
//! (render `box.py`, run `freecadcmd`, verify artifacts and `report.json`)
//! with the default output `build/coupon`, and prints the calibration
//! reminder.
//!
//! The geometry checks need FreeCAD and follow the existing env-gated
//! pattern: they skip on a native host unless `BOXGEN_REQUIRE_FREECAD=1` is
//! set (the Docker gate and CI set it). The golden-script, fake-launcher, and
//! schema tests are pure Rust and always run.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use predicates::prelude::*;
use serde_json::Value;
use tempfile::tempdir;

/// Comparison tolerance for exact millimetre values (faces, wire boxes).
const EPS: f64 = 1e-6;

/// Comparison tolerance for slice-derived areas.
const AREA_TOLERANCE: f64 = 0.05;

/// Comparison tolerance for bisection-measured faces (0.01 mm probe).
const PROBE_TOLERANCE: f64 = 0.02;

/// The pinned coupon numbers from calibration section 2.
const COUPON_ID: &str = "coupon-1x1-6U-v1";
const COUPON_OUTER_MM: f64 = 41.5;
const COUPON_HEIGHT_MM: f64 = 42.0;
const T_DIVIDER_THICKNESS_MM: f64 = 2.0;
const T_DIVIDER_LIP_GAP_MM: f64 = 10.0;
const T_DIVIDER_LENGTH_MM: f64 = 20.0;
const LID_RIB_INTERFERENCE_MM: f64 = 0.15;
const FINGER_HOLE_DIAMETER_MM: f64 = 20.0;

/// Runs `boxgen coupon --out <out>` and returns its output.
fn coupon_into(out: &Path) -> Output {
    common::boxgen()
        .args(["coupon", "--out"])
        .arg(out)
        .output()
        .expect("boxgen coupon runs")
}

/// Reads a wire bounding box as `[x_min, y_min, x_max, y_max]`.
fn wire_box(report: &Value, path: &str) -> [f64; 4] {
    let list = common::value(report, path)
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

/// The coupon script is the same generated `box.py` as every build: a pure
/// function of the internal spec, pinned byte for byte, importing the master.
#[test]
fn the_coupon_golden_script_is_a_pure_function_of_the_spec() {
    let spec = boxgen::build::coupon_spec();
    let rendered = boxgen::build::render_box_py(&spec);

    assert!(
        rendered.contains("import cad.build as master"),
        "the coupon must run the same master: {rendered}"
    );
    assert!(rendered.contains("\"coupon\": true"), "{rendered}");
    assert_eq!(
        rendered,
        include_str!("golden/coupon.box.py"),
        "generated box.py differs from the golden snapshot; update \
         tests/golden/coupon.box.py only for an intentional change"
    );
}

/// The coupon flag is internal: the closed user schema rejects it like any
/// unknown field, and both commands exit 2 before FreeCAD starts.
#[test]
fn a_user_spec_with_the_coupon_field_is_rejected() {
    let temp = tempdir().expect("tempdir");
    let spec = "\
system: SYS-1.0
id: 1x1-6U-coupon-user-v1
footprint: [1, 1]
height_u: 6
lid: true
coupon: true
divisions: { compartment: Coupon }
";
    let path = temp.path().join("spec.yaml");
    fs::write(&path, spec).expect("write spec");

    common::boxgen()
        .arg("validate")
        .arg(&path)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unknown field").and(predicate::str::contains("coupon")));

    common::boxgen()
        .args(["build"])
        .arg(&path)
        .arg("--out")
        .arg(temp.path().join("out"))
        .env("BOXGEN_FREECADCMD", "/nonexistent/freecadcmd")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unknown field").and(predicate::str::contains("coupon")));
}

/// Without `--out`, the coupon lands in `build/coupon` (relative to the cwd)
/// and runs the normal pipeline: render, run the launcher, verify artifacts,
/// rewrite the report, print the summary and the calibration reminder.
#[test]
fn coupon_defaults_to_build_coupon_and_reuses_the_build_pipeline() {
    let temp = tempdir().expect("tempdir");
    let launcher = launcher_with_coupon_artifacts(temp.path());

    let output = common::boxgen()
        .arg("coupon")
        .current_dir(temp.path())
        .env("BOXGEN_FREECADCMD", &launcher)
        .output()
        .expect("boxgen coupon runs");
    common::assert_success(&output);

    let out = temp.path().join("build/coupon");
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
    assert!(
        out.join("cad/build.py").is_file(),
        "the coupon build must copy the master like every build"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("id: coupon-1x1-6U-v1"), "{stdout}");
    assert!(stdout.contains("output: build/coupon"), "{stdout}");
    assert!(stdout.contains("print two coupons and one lid"), "{stdout}");
    assert!(stdout.contains("docs/spec/docs/calibration.md"), "{stdout}");
    assert!(stdout.contains("nine tests"), "{stdout}");

    // The Rust side rewrote report.json with the merged warnings and verified
    // the coupon section.
    let report: Value =
        serde_json::from_str(&fs::read_to_string(out.join("report.json")).expect("report"))
            .expect("report.json is JSON");
    assert!(common::flag(&report, "coupon.t_divider"), "{report}");
    assert_eq!(
        report.pointer("/id").and_then(Value::as_str),
        Some(COUPON_ID)
    );
}

/// The full coupon build under the pinned FreeCAD: exit 0, the complete
/// artifact set, the report invariants, and the calibration reminder.
#[test]
fn coupon_builds_end_to_end() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let out = temp.path().join("build/coupon");
    let output = coupon_into(&out);
    common::assert_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("id: coupon-1x1-6U-v1"), "{stdout}");
    assert!(stdout.contains("outer: 41.5 x 41.5 x 42.0 mm"), "{stdout}");
    assert!(stdout.contains("interior depth: 33.25 mm"), "{stdout}");
    assert!(stdout.contains("ribs: yes"), "{stdout}");
    assert!(stdout.contains("warnings: none"), "{stdout}");
    assert!(stdout.contains("print two coupons and one lid"), "{stdout}");
    assert!(
        stdout.contains("nine tests in docs/spec/docs/calibration.md"),
        "{stdout}"
    );

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

    let generated = fs::read_to_string(out.join("box.py")).expect("box.py is readable");
    assert_eq!(
        generated,
        include_str!("golden/coupon.box.py"),
        "the coupon build wrote a script that differs from the golden snapshot"
    );

    let report: Value =
        serde_json::from_str(&fs::read_to_string(out.join("report.json")).expect("report"))
            .expect("report.json is JSON");

    // Calibration section 2: 1 x 1, 6U, standard shell, ribbed lid.
    assert_eq!(report["id"].as_str(), Some(COUPON_ID));
    common::assert_near(
        common::number(&report, "dimensions.outer_x_mm"),
        COUPON_OUTER_MM,
        0.2,
        "outer x",
    );
    common::assert_near(
        common::number(&report, "dimensions.outer_y_mm"),
        COUPON_OUTER_MM,
        0.2,
        "outer y",
    );
    common::assert_near(
        common::number(&report, "dimensions.height_mm"),
        COUPON_HEIGHT_MM,
        0.2,
        "height",
    );
    // Standard §4.2: the report carries the depth measured from the model
    // (floor top to lip bottom), not the design formula; 0.05 mm covers the
    // kernel's section tolerance.
    common::assert_near(
        common::number(&report, "interior_depth_mm"),
        33.25,
        0.05,
        "interior depth",
    );

    assert_eq!(common::number(&report, "body.solids"), 1.0);
    assert!(common::flag(&report, "body.is_valid"), "{report}");
    assert!(common::flag(&report, "body.is_closed"), "{report}");
    common::assert_close(
        common::number(&report, "body.floor_fillet_mm"),
        2.0,
        "floor fillet",
    );
    assert!(common::flag(&report, "stl.is_solid"), "{report}");

    // The single-leaf tree adds no tree dividers; the T-divider is the
    // coupon section, fixed at 2.0 mm with its top at 28.0.
    assert_eq!(common::number(&report, "dividers.count"), 0.0);
    assert!(common::flag(&report, "coupon.t_divider"), "{report}");
    assert_eq!(report["coupon"]["wall"].as_str(), Some("-y"));
    common::assert_close(
        common::number(&report, "coupon.length_mm"),
        T_DIVIDER_LENGTH_MM,
        "T-divider length",
    );
    common::assert_close(
        common::number(&report, "coupon.thickness_mm"),
        T_DIVIDER_THICKNESS_MM,
        "T-divider thickness",
    );
    common::assert_close(
        common::number(&report, "coupon.top_mm"),
        COUPON_HEIGHT_MM - 4.0 - T_DIVIDER_LIP_GAP_MM,
        "T-divider top (10 mm below the lip)",
    );
    common::assert_close(
        common::number(&report, "coupon.top_chamfer_mm"),
        0.4,
        "T-divider top chamfer",
    );

    // The ribbed lid: one valid closed solid, 37.1 x 37.1, z 37.0..41.0.
    assert_eq!(common::number(&report, "lid.solids"), 1.0);
    assert!(common::flag(&report, "lid.is_valid"), "{report}");
    assert!(common::flag(&report, "lid.is_closed"), "{report}");
    assert!(common::flag(&report, "lid.ribs"), "{report}");
    common::assert_close(
        common::number(&report, "lid.roundover_mm"),
        1.0,
        "lid roundover",
    );
    common::assert_close(
        common::number(&report, "lid.bbox.x_max") - common::number(&report, "lid.bbox.x_min"),
        37.1,
        "lid plate width",
    );
    common::assert_close(
        common::number(&report, "lid.bbox.z_min"),
        37.0,
        "lid skirt bottom",
    );
    common::assert_close(common::number(&report, "lid.bbox.z_max"), 41.0, "lid top");
    assert!(common::flag(&report, "lid_stl.is_solid"), "{report}");

    assert!(
        report["warnings"].as_array().is_some_and(Vec::is_empty),
        "the coupon must be warning-free: {report}"
    );
}

/// Runs `cad/tests/coupon.py` under `freecadcmd` once and parses its JSON.
fn geometry_report() -> &'static Value {
    use std::sync::OnceLock;

    static REPORT: OnceLock<Value> = OnceLock::new();

    REPORT.get_or_init(|| common::script_report("coupon.py"))
}

/// The built coupon body is one valid closed solid with the calibration's
/// outer dimensions, T-divider, junction coves, and top chamfer.
#[test]
fn the_coupon_body_carries_the_calibration_features() {
    if !common::freecad_required() {
        return;
    }
    let report = geometry_report();

    assert!(common::flag(report, "module_imported"));
    common::assert_close(
        common::number(report, "constants.COUPON_DIVIDER_LENGTH"),
        20.0,
        "coupon divider length",
    );
    assert_eq!(
        common::value(report, "constants.COUPON_DIVIDER_WALL").as_str(),
        Some("-y"),
    );
    common::assert_close(
        common::number(report, "constants.DIVIDER_THICKNESS"),
        2.0,
        "divider thickness",
    );
    common::assert_close(
        common::number(report, "constants.DIVIDER_TOP_CHAMFER"),
        0.4,
        "divider top chamfer",
    );
    common::assert_close(
        common::number(report, "constants.JUNCTION_FILLET"),
        2.0,
        "junction cove radius",
    );
    common::assert_close(
        common::number(report, "constants.OUTER_WIDTH_MM"),
        COUPON_OUTER_MM,
        "outer width",
    );
    common::assert_close(
        common::number(report, "constants.HEIGHT_MM"),
        COUPON_HEIGHT_MM,
        "height",
    );
    common::assert_close(
        common::number(report, "constants.DIVIDER_GAP_MM"),
        T_DIVIDER_LIP_GAP_MM,
        "divider gap below the lip",
    );

    // One valid closed solid at the calibration's outer dimensions.
    assert_eq!(common::number(report, "body.solids"), 1.0);
    assert_eq!(common::number(report, "body.shells"), 1.0);
    assert!(common::flag(report, "body.is_valid"));
    assert!(common::flag(report, "body.is_closed"));
    common::assert_close(
        common::number(report, "body.bbox.x_mm"),
        41.5,
        "body bbox x",
    );
    common::assert_close(
        common::number(report, "body.bbox.y_mm"),
        41.5,
        "body bbox y",
    );
    common::assert_close(
        common::number(report, "body.bbox.z_mm"),
        42.0,
        "body bbox z",
    );

    // The T-divider: 2.0 mm thick, fused to the floor and the -y wall,
    // 20 mm from the wall face, top at 28.0 (10 mm below the 38.0 lip).
    common::assert_near(
        common::number(report, "body.top_probe_mm"),
        28.0,
        PROBE_TOLERANCE,
        "T-divider top",
    );
    common::assert_close(common::number(report, "body.z_floor_mm"), 4.75, "floor top");
    common::assert_close(
        common::number(report, "body.divider_top_mm"),
        28.0,
        "divider top",
    );
    common::assert_close(
        common::number(report, "body.wall_face_mm"),
        -18.75,
        "wall face",
    );
    common::assert_near(
        common::number(report, "body.near_face_mm"),
        -1.0,
        PROBE_TOLERANCE,
        "near face",
    );
    common::assert_near(
        common::number(report, "body.far_face_mm"),
        1.0,
        PROBE_TOLERANCE,
        "far face",
    );
    common::assert_near(
        common::number(report, "body.far_face_mm") - common::number(report, "body.near_face_mm"),
        T_DIVIDER_THICKNESS_MM,
        PROBE_TOLERANCE,
        "T-divider thickness",
    );
    common::assert_near(
        common::number(report, "body.free_end_mm"),
        1.25,
        PROBE_TOLERANCE,
        "free end",
    );
    common::assert_near(
        common::number(report, "body.free_end_mm") - common::number(report, "body.wall_face_mm"),
        T_DIVIDER_LENGTH_MM,
        PROBE_TOLERANCE,
        "T-divider length from the wall",
    );

    // Slice at z = 20: divider material; at z = 29: none. The area difference
    // is the 2 x 20 mm plate plus the two quarter-round wall columns.
    assert!(common::flag(report, "body.material_at_z20"));
    assert!(!common::flag(report, "body.material_at_z29"));
    let z20 = common::number(report, "body.section_z20.area_mm2");
    let z29 = common::number(report, "body.section_z29.area_mm2");
    let expected = common::number(report, "expected_slice_area_difference_mm2");
    common::assert_near(z20 - z29, expected, AREA_TOLERANCE, "slice area difference");
    assert!(
        z20 > z29,
        "the divider material must be present at z = 20 only: {z20} vs {z29}"
    );

    // The 2.0 mm floor cove: material at the expected quarter-round boundary
    // 0.1 and 0.75 above the floor, void beyond it and above the cove, on
    // both plate faces, and nothing beside the plate past its free end.
    assert!(common::flag(report, "body.cove_floor_material_0_1"));
    assert!(common::flag(report, "body.cove_floor_void_0_1"));
    assert!(common::flag(report, "body.cove_floor_material_0_75"));
    assert!(common::flag(report, "body.cove_floor_void_0_75"));
    assert!(common::flag(report, "body.cove_floor_void_at_top"));
    assert!(common::flag(report, "body.cove_floor_material_0_1_far"));
    assert!(common::flag(report, "body.cove_floor_void_0_1_far"));
    assert!(common::flag(report, "body.cove_floor_void_past_free_end"));

    // The 2.0 mm wall cove columns at both plate faces.
    assert!(common::flag(report, "body.cove_wall_material"));
    assert!(common::flag(report, "body.cove_wall_void"));
    assert!(common::flag(report, "body.cove_wall_material_far"));
    assert!(common::flag(report, "body.cove_wall_void_far"));

    // The 0.4 mm top chamfer: 0.3 clear per side 0.1 below the top, flush
    // 0.5 below, and 0.3 short at the free end.
    common::assert_near(
        common::number(report, "body.chamfer_near_mm"),
        -0.7,
        PROBE_TOLERANCE,
        "chamfer near face",
    );
    common::assert_near(
        common::number(report, "body.chamfer_far_mm"),
        0.7,
        PROBE_TOLERANCE,
        "chamfer far face",
    );
    common::assert_near(
        common::number(report, "body.chamfer_free_end_mm"),
        0.95,
        PROBE_TOLERANCE,
        "chamfer free end",
    );
    common::assert_near(
        common::number(report, "body.flush_near_mm"),
        -1.0,
        PROBE_TOLERANCE,
        "flush near face",
    );
    common::assert_near(
        common::number(report, "body.flush_far_mm"),
        1.0,
        PROBE_TOLERANCE,
        "flush far face",
    );
    common::assert_near(
        common::number(report, "body.flush_free_end_mm"),
        1.25,
        PROBE_TOLERANCE,
        "flush free end",
    );
}

/// The coupon lid is the standard ribbed 1 x 1 x 6U lid: 37.1 x 37.1 mm,
/// z 37.0..41.0, 20 mm finger hole 14 mm from the +y edge, ribs adding
/// exactly 0.30 mm to the pilot width.
#[test]
fn the_coupon_lid_is_the_standard_ribbed_lid() {
    if !common::freecad_required() {
        return;
    }
    let report = geometry_report();

    assert_eq!(common::number(report, "lid.solids"), 1.0);
    assert!(common::flag(report, "lid.is_valid"));
    assert!(common::flag(report, "lid.is_closed"));
    common::assert_close(
        common::number(report, "lid.bbox.x_mm"),
        37.1,
        "lid plate width",
    );
    common::assert_close(
        common::number(report, "lid.bbox.y_mm"),
        37.1,
        "lid plate depth",
    );
    common::assert_close(
        common::number(report, "lid.bbox.z_min_mm"),
        37.0,
        "skirt bottom",
    );
    common::assert_close(common::number(report, "lid.bbox.z_max_mm"), 41.0, "lid top");
    assert!(common::flag(report, "lid.roundover_applied"));

    // The plate section just above the plate bottom carries the plate and the
    // 20 mm finger hole centred 14 mm from the +y edge (y = 6.75).
    common::assert_close(
        wire_box(report, "lid.plate_section.wires.0")[2]
            - wire_box(report, "lid.plate_section.wires.0")[0],
        FINGER_HOLE_DIAMETER_MM,
        "finger hole diameter",
    );
    let hole = wire_box(report, "lid.plate_section.wires.0");
    common::assert_close(hole[0], -10.0, "finger hole x_min");
    common::assert_close(hole[2], 10.0, "finger hole x_max");
    common::assert_close(hole[1], -3.25, "finger hole y_min");
    common::assert_close(hole[3], 16.75, "finger hole y_max");
    common::assert_close(
        common::number(report, "lid.expected.hole_centre_y_mm"),
        6.75,
        "finger hole centre y",
    );
    common::assert_close(
        wire_box(report, "lid.plate_section.wires.1")[2]
            - wire_box(report, "lid.plate_section.wires.1")[0],
        37.1,
        "plate width",
    );

    // The ribs halve the 0.30 mm pilot clearance locally: the pilot grows by
    // exactly 2 x 0.15 mm, one rib per side.
    let plain = wire_box(report, "lid.pilot_plain.wires.1");
    let ribbed = wire_box(report, "lid.pilot_ribbed.wires.1");
    common::assert_close(plain[2] - plain[0], 35.7, "plain pilot width");
    common::assert_close(
        ribbed[2] - ribbed[0] - (plain[2] - plain[0]),
        2.0 * LID_RIB_INTERFERENCE_MM,
        "rib interference",
    );
    common::assert_close(
        common::number(report, "lid.pilot_ribbed.area_mm2")
            - common::number(report, "lid.pilot_plain.area_mm2"),
        2.0 * 10.0 * LID_RIB_INTERFERENCE_MM,
        "rib cross-section area",
    );
}

/// The exported `box.step` carries the T-divider: the slice at z = 20
/// encloses 41.7168 mm2 less void than z = 29 (the 2 x 20 mm plate plus the
/// two quarter-round wall columns), and the body is one closed solid.
#[test]
fn the_exported_coupon_step_slices_show_the_t_divider() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempdir().expect("tempdir");
    let out = temp.path().join("build/coupon");
    common::assert_success(&coupon_into(&out));

    let slices = common::slice_step(&out.join("box.step"), "20,29");
    assert_eq!(
        slices["objects"],
        serde_json::json!(["Body", "Lid"]),
        "the coupon STEP carries Body and Lid: {slices}"
    );
    assert_eq!(slices["solids"].as_u64(), Some(1));
    assert_eq!(slices["is_closed"].as_bool(), Some(true));

    let areas = |key: &str| -> Vec<f64> {
        slices[key]
            .as_array()
            .unwrap_or_else(|| panic!("slice report has no {key}: {slices}"))
            .iter()
            .map(|area| area.as_f64().expect("area"))
            .collect()
    };

    // Outer face area: 41.5^2 minus the four 3.75 mm corner bites.
    let outer = 41.5 * 41.5 - (4.0 - std::f64::consts::PI) * 3.75 * 3.75;
    // Interior void: 37.5^2 minus the four 1.75 mm corner bites.
    let interior = 37.5 * 37.5 - (4.0 - std::f64::consts::PI) * 1.75 * 1.75;
    // The divider material at z = 20: plate plus the two wall columns.
    let divider = 2.0 * T_DIVIDER_LENGTH_MM + 2.0 * (4.0 - std::f64::consts::PI);

    let z20 = areas("section_z20_areas");
    let z29 = areas("section_z29_areas");
    assert_eq!(z20.len(), 2, "z = 20 wires: {z20:?}");
    assert_eq!(z29.len(), 2, "z = 29 wires: {z29:?}");
    common::assert_near(
        z20[0],
        interior - divider,
        AREA_TOLERANCE,
        "z = 20 void area",
    );
    common::assert_near(z29[0], interior, AREA_TOLERANCE, "z = 29 void area");
    common::assert_near(z20[1], outer, AREA_TOLERANCE, "z = 20 outer area");
    common::assert_near(z29[1], outer, AREA_TOLERANCE, "z = 29 outer area");

    // And the outer wire boxes pin the 41.5 x 41.5 footprint in the artifact.
    let outer_box = wire_box(&slices, "section_z20.0");
    common::assert_close(outer_box[0], -20.75, "outer x_min");
    common::assert_close(outer_box[2], 20.75, "outer x_max");
    common::assert_close(outer_box[1], -20.75, "outer y_min");
    common::assert_close(outer_box[3], 20.75, "outer y_max");
}

/// Same spec + same master = the same coupon: `box.py`, `body.stl`,
/// `lid.stl`, and `report.json` are byte-identical, and `box.step` matches
/// after normalising its `FILE_NAME` timestamp.
#[test]
fn two_coupon_builds_are_byte_identical_apart_from_the_step_timestamp() {
    if !common::freecad_required() {
        return;
    }

    let first = tempdir().expect("first tempdir");
    let second = tempdir().expect("second tempdir");
    let out_a = first.path().join("out");
    let out_b = second.path().join("out");

    common::assert_success(&coupon_into(&out_a));
    common::assert_success(&coupon_into(&out_b));

    for name in ["box.py", "body.stl", "lid.stl", "report.json"] {
        let a = fs::read(out_a.join(name)).expect("first artifact");
        let b = fs::read(out_b.join(name)).expect("second artifact");
        assert_eq!(a, b, "`{name}` differs between two identical coupon builds");
    }

    let step_a = fs::read_to_string(out_a.join("box.step")).expect("first STEP");
    let step_b = fs::read_to_string(out_b.join("box.step")).expect("second STEP");
    assert_eq!(
        common::normalise_step(&step_a),
        common::normalise_step(&step_b),
        "the coupon STEP differs beyond the FILE_NAME timestamp"
    );
}

/// The contract exit codes stay in place for the coupon too: a missing
/// launcher is an environment failure (4), a failing launcher is a build
/// failure (3).
#[test]
fn coupon_uses_the_contract_exit_codes() {
    let temp = tempdir().expect("tempdir");

    common::boxgen()
        .arg("coupon")
        .arg("--out")
        .arg(temp.path().join("out"))
        .env("BOXGEN_FREECADCMD", "/nonexistent/freecadcmd")
        .assert()
        .code(4)
        .stderr(
            predicate::str::contains("BOXGEN_FREECADCMD")
                .and(predicate::str::contains("FreeCAD 1.0.0")),
        );

    let launcher = common::fake_launcher(
        temp.path(),
        "if [ \"$1\" = \"--version\" ]; then echo 'FreeCAD 1.0.0 Revision:'; exit 0; fi\n\
         echo 'simulated coupon failure' >&2\n\
         exit 1",
    );
    common::boxgen()
        .arg("coupon")
        .arg("--out")
        .arg(temp.path().join("out"))
        .env("BOXGEN_FREECADCMD", &launcher)
        .assert()
        .code(3)
        .stderr(
            predicate::str::contains("freecadcmd exited with")
                .and(predicate::str::contains("simulated coupon failure")),
        );
}

/// The SYS-1.0 spec repository is vendored at `docs/spec/`.
fn spec_repository() -> Option<PathBuf> {
    let candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/spec");
    candidate
        .join("docs/calibration.md")
        .is_file()
        .then_some(candidate)
}

/// The coupon's fixed numbers come from calibration section 2, and the
/// document still states them. The spec is vendored at `docs/spec/`, so the
/// comparison always runs; it skips only if that directory is missing.
#[test]
fn the_coupon_feature_set_matches_the_calibration_document() {
    // The spec's numbers, derived from the internal spec and the standard's
    // constants, must equal the calibration document's table.
    let spec = boxgen::build::coupon_spec();
    let body = spec.body();
    assert_eq!(spec.footprint, [1, 1]);
    assert_eq!(spec.height_u, 6);
    assert!((body.outer_width_mm - COUPON_OUTER_MM).abs() < EPS);
    assert!((body.total_height_mm - COUPON_HEIGHT_MM).abs() < EPS);
    let divider_top = boxgen::tree::BASE_HEIGHT_MM + body.divider_height_mm;
    let lip_bottom = body.total_height_mm - boxgen::tree::LIP_HEIGHT_MM;
    assert!(
        (lip_bottom - divider_top - T_DIVIDER_LIP_GAP_MM).abs() < EPS,
        "the divider top must sit 10 mm below the lip"
    );

    let Some(repository) = spec_repository() else {
        eprintln!("skipping calibration-document comparison: docs/spec/ is missing");
        return;
    };
    let document =
        fs::read_to_string(repository.join("docs/calibration.md")).expect("calibration doc");

    let row = |prefix: &str| -> String {
        document
            .lines()
            .find(|line| line.trim_start().starts_with(prefix))
            .unwrap_or_else(|| panic!("calibration.md has no {prefix} row"))
            .to_owned()
    };

    let footprint = row("| Footprint |");
    assert!(
        footprint.contains("41.5") && footprint.contains("1 × 1"),
        "{footprint}"
    );
    let height = row("| Height |");
    assert!(height.contains("6U") && height.contains("42"), "{height}");
    let divider = row("| Divider |");
    assert!(
        divider.contains("2.0 mm") && divider.contains("10 mm below the lip"),
        "{divider}"
    );
    let lid = row("| Lid |");
    assert!(lid.contains("0.15 mm") && lid.contains("20 mm"), "{lid}");
    assert!(
        document.contains("two coupons and one lid"),
        "the calibration document must keep the print-two-coupons instruction"
    );
}

/// A complete coupon `report.json` for the fake launcher: the lid sections
/// plus the calibration T-divider section the Rust validation requires.
fn fake_coupon_report() -> String {
    serde_json::to_string(&serde_json::json!({
        "id": COUPON_ID,
        "dimensions": {"outer_x_mm": 41.5, "outer_y_mm": 41.5, "height_mm": 42.0},
        "interior_depth_mm": 33.25,
        "body": {
            "volume_mm3": 16105.32,
            "area_mm2": 1.0,
            "faces": 1,
            "solids": 1,
            "is_valid": true,
            "is_closed": true,
            "floor_fillet_mm": 2.0,
        },
        "lid": {
            "volume_mm3": 2584.66,
            "area_mm2": 1.0,
            "faces": 1,
            "solids": 1,
            "is_valid": true,
            "is_closed": true,
            "roundover_mm": 1.0,
            "ribs": true,
            "bbox": {
                "x_min": -18.55, "x_max": 18.55,
                "y_min": -18.55, "y_max": 18.55,
                "z_min": 37.0, "z_max": 41.0,
            },
        },
        "coupon": {
            "t_divider": true,
            "wall": "-y",
            "length_mm": 20.0,
            "thickness_mm": 2.0,
            "height_above_floor_mm": 23.25,
            "top_mm": 28.0,
            "top_chamfer_mm": 0.4,
        },
        "stl": {"facets": 1, "is_solid": true, "volume_mm3": 1.0},
        "lid_stl": {"facets": 1, "is_solid": true, "volume_mm3": 1.0},
        "artifacts": {"box_step_bytes": 1, "body_stl_bytes": 1, "lid_stl_bytes": 1},
        "mass_g": 23.7,
        "print_time_h": 0.9,
        "warnings": [],
    }))
    .expect("a JSON value serializes")
}

/// A launcher that passes the version check and writes every coupon artifact.
fn launcher_with_coupon_artifacts(dir: &Path) -> PathBuf {
    let report_path = dir.join("report-source.json");
    fs::write(&report_path, fake_coupon_report()).expect("write report source");
    common::fake_launcher(
        dir,
        &format!(
            "if [ \"$1\" = \"--version\" ]; then echo 'FreeCAD 1.0.0 Revision:'; exit 0; fi\n\
             out=$(dirname \"$1\")\n\
             printf 'STEP' > \"$out/box.step\"\n\
             printf 'BODY' > \"$out/body.stl\"\n\
             printf 'LID' > \"$out/lid.stl\"\n\
             printf 'FCSTD' > \"$out/box.FCStd\"\n\
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
