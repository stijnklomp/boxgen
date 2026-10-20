//! SYS-2.0 corner radii and internal fillet contract (ticket #7).
//!
//! `cad/fillets.py` applies the standard's reduce-then-fail rule (2.0 mm,
//! then 1.0 mm, then a geometry error -- never a sharp internal corner above
//! the base) and `cad/shell.py` uses it for the floor-to-wall cove.
//! `cad/tests/radii.py` measures the helper and slices the built 1x1x6U and
//! 1x1x3U bodies; this test asserts the SYS-2.0 numbers.
//!
//! Why synthetic prisms for the reduce and failure paths: the real 1x1 3U
//! interior cutter -- the smallest legal interior -- still takes the 2.0 mm
//! fillet (asserted below), so no legal spec can force the fallback. A
//! 10x10x1.5 mm prism forces reduce (a 2.0 mm fillet cannot attach to a
//! 1.5 mm tall bottom edge, a 1.0 mm fillet can) and a 10x10x0.5 mm prism
//! forces the failure (both radii exceed the edge height).
//!
//! Like `tests/gridfinity_interface.rs`, this skips on a native host without
//! FreeCAD. The Docker gate and CI set `BOXGEN_REQUIRE_FREECAD=1`, where a
//! missing `freecadcmd` or a drifting radius is a hard failure.

mod common;

use std::sync::OnceLock;

use serde_json::Value;

/// The measured 1x1x6U body volume under the pinned FreeCAD, with the solid
/// base zone of ADR-0013 and the 0.8 mm floor slab of ADR-0014. The test
/// allows +/- 3% for a geometry-kernel drift, like `tests/build.rs`.
const EXPECTED_VOLUME_6U_MM3: f64 = 20_759.282137;

/// The measured 1x1x3U boundary body volume (the smallest legal interior).
const EXPECTED_VOLUME_3U_MM3: f64 = 14_321.574233;

/// Runs `cad/tests/radii.py` under `freecadcmd` once and parses its JSON.
fn report() -> &'static Value {
    static REPORT: OnceLock<Value> = OnceLock::new();

    REPORT.get_or_init(|| common::script_report("radii.py"))
}

/// True when a raw `makeFillet` attempt raised or produced an unacceptable
/// shape (the helper's acceptance rule: valid, one solid, positive volume).
fn attempt_failed(report: &Value, path: &str) -> bool {
    let attempt = common::value(report, path);
    if attempt.get("exception").is_some() {
        return true;
    }
    !(attempt["valid"].as_bool().unwrap_or(false)
        && attempt["solids"].as_u64() == Some(1)
        && attempt["volume_mm3"].as_f64().unwrap_or(0.0) > 0.0)
}

/// The helper's inputs are the standard's radii, mirrored from the master.
#[test]
fn fillet_helper_constants_pin_the_standard() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(
        common::value(report, "module_imported").as_bool(),
        Some(true),
        "cad/tests/radii.py did not import"
    );
    common::assert_close(common::number(report, "constants.FLOOR"), 0.8, "floor slab");
    common::assert_close(
        common::number(report, "constants.FLOOR_TOP"),
        5.55,
        "floor top",
    );
    common::assert_close(
        common::number(report, "constants.FLOOR_FILLET"),
        2.0,
        "standard floor fillet",
    );
    common::assert_close(
        common::number(report, "constants.FILLET_FALLBACK"),
        1.0,
        "fillet fallback",
    );
    common::assert_close(
        common::number(report, "constants.WALL"),
        2.0,
        "wall thickness",
    );
    common::assert_close(
        common::number(report, "constants.INNER_RADIUS"),
        1.75,
        "inner corner radius",
    );
    common::assert_close(
        common::number(report, "constants.OUTER_RADIUS"),
        3.75,
        "outer corner radius",
    );
}

/// The report's interior depth is measured from the built model, not from the
/// design formula (standard §4.2: "The generator reports the exact usable
/// depth from the model; the table is the design figure"): the floor top and
/// the lip bottom are found on the shape, so 15.45 / 36.45 / 57.45 / 78.45 mm
/// for 3U / 6U / 9U / 12U within the 0.05 mm section tolerance.
#[test]
fn the_interior_depth_is_measured_from_the_model() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    for (height_u, depth) in [(3, 15.45), (6, 36.45), (9, 57.45), (12, 78.45)] {
        let path = format!("interior_depth.{height_u}");
        common::assert_near(
            common::number(report, &format!("{path}.measured_mm")),
            depth,
            0.05,
            &format!("{height_u}U measured interior depth"),
        );
        common::assert_close(
            common::number(report, &format!("{path}.design_mm")),
            depth,
            &format!("{height_u}U design interior depth"),
        );
    }
}

/// The smallest legal interior still takes the standard 2.0 mm radius, which
/// is why the reduce path is exercised on a synthetic prism instead.
#[test]
fn the_standard_radius_is_used_on_the_smallest_legal_interior() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert!(
        !attempt_failed(report, "helper_standard.standard_attempt"),
        "the 1x1 3U interior cutter must accept the 2.0 mm fillet"
    );
    assert_eq!(
        common::value(report, "helper_standard.result.raised").as_bool(),
        Some(false),
        "the helper must not raise for a legal 3U interior"
    );
    common::assert_close(
        common::number(report, "helper_standard.result.used_mm"),
        2.0,
        "accepted floor fillet radius",
    );
    assert_eq!(
        common::value(report, "helper_standard.result.solids").as_u64(),
        Some(1)
    );
    assert!(common::number(report, "helper_standard.result.volume_mm3") > 0.0);
}

/// A shape where 2.0 mm fails and 1.0 mm succeeds returns exactly 1.0 mm.
#[test]
fn a_thin_edge_reduces_to_the_fallback_radius() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert!(
        attempt_failed(report, "helper_reduce.standard_attempt"),
        "the reduce path must be forced: the 2.0 mm fillet must fail on the 1.5 mm edge"
    );
    assert!(
        !attempt_failed(report, "helper_reduce.fallback_attempt"),
        "the 1.0 mm fillet must succeed on the 1.5 mm edge"
    );
    assert_eq!(
        common::value(report, "helper_reduce.result.raised").as_bool(),
        Some(false)
    );
    common::assert_close(
        common::number(report, "helper_reduce.result.used_mm"),
        1.0,
        "reduced fillet radius",
    );
    assert_eq!(
        common::value(report, "helper_reduce.result.solids").as_u64(),
        Some(1)
    );
    let reduced = common::number(report, "helper_reduce.result.volume_mm3");
    let source = common::number(report, "helper_reduce.source_volume_mm3");
    assert!(
        reduced > 0.0 && reduced < source,
        "the reduced fillet must remove material: {reduced} mm3 of {source} mm3"
    );
}

/// When both radii fail the helper raises instead of returning a sharp shape.
#[test]
fn when_both_radii_fail_the_helper_raises_a_geometry_error() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert!(
        attempt_failed(report, "helper_failure.standard_attempt"),
        "both attempts must fail on the 0.5 mm edge"
    );
    assert!(attempt_failed(report, "helper_failure.fallback_attempt"));
    assert_eq!(
        common::value(report, "helper_failure.result.raised").as_bool(),
        Some(true),
        "the helper must raise when both radii fail"
    );
    assert_eq!(
        common::value(report, "helper_failure.result.error_type").as_str(),
        Some("GeometryError")
    );
    let message = common::value(report, "helper_failure.result.message")
        .as_str()
        .expect("error message");
    assert!(
        message.contains("2.0") && message.contains("1.0") && message.contains("sharp"),
        "the error must name both radii and refuse a sharp corner: {message}"
    );
}

/// The golden 1x1x6U body: dimensions, volume, 3.75 outer and 1.75 inner
/// vertical corner arcs at mid height.
#[test]
fn the_6u_body_has_the_standard_corner_radii() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(common::value(report, "body_6u.solids").as_u64(), Some(1));
    assert_eq!(
        common::value(report, "body_6u.is_valid").as_bool(),
        Some(true),
        "the body must be a valid shape"
    );
    common::assert_near(
        common::number(report, "body_6u.outer_x_mm"),
        41.5,
        0.2,
        "outer x",
    );
    common::assert_near(
        common::number(report, "body_6u.outer_y_mm"),
        41.5,
        0.2,
        "outer y",
    );
    common::assert_near(
        common::number(report, "body_6u.height_mm"),
        46.4,
        0.2,
        "height",
    );

    let volume = common::number(report, "body_6u.volume_mm3");
    common::assert_near(
        volume,
        EXPECTED_VOLUME_6U_MM3,
        EXPECTED_VOLUME_6U_MM3 * 0.03,
        "body volume",
    );
    common::assert_close(
        common::number(report, "body_6u.floor_fillet_used_mm"),
        2.0,
        "floor fillet recorded in the build result",
    );

    // Smallest width first: the inner cavity wire, then the outer body wire.
    common::assert_close(
        common::number(report, "body_6u.sections.mid_body.0.width_mm"),
        37.5,
        "inner cavity width at mid height",
    );
    common::assert_close_list(
        common::value(report, "body_6u.sections.mid_body.0.radii"),
        &[1.75],
        "inner vertical corner radius",
    );
    common::assert_close(
        common::number(report, "body_6u.sections.mid_body.1.width_mm"),
        41.5,
        "outer width at mid height",
    );
    common::assert_close_list(
        common::value(report, "body_6u.sections.mid_body.1.radii"),
        &[3.75],
        "outer vertical corner radius",
    );
}

/// Just above the floor the cavity is narrower than the straight cavity
/// (the cove exists) and the vertical section shows the 2.0 mm arc.
#[test]
fn the_floor_cove_is_a_2mm_fillet() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    // Analytic width at 0.25 mm above the floor for a 2.0 mm radius cove:
    // 37.5 - 2 * (2 - sqrt(2^2 - (2 - 0.25)^2)) = 35.4365 mm.
    let width = common::number(report, "body_6u.sections.just_above_floor.0.width_mm");
    assert!(
        width < 37.5 - 1.5,
        "the cove must narrow the cavity just above the floor: {width} mm"
    );
    common::assert_near(
        width,
        35.4365,
        0.01,
        "cove inner width just above the floor",
    );
    common::assert_close_list(
        common::value(report, "body_6u.cove_vertical_radii"),
        &[2.0],
        "floor cove radius in the vertical centre section",
    );
}

/// No circular edge above the base is sharper than the 1.0 mm contract
/// minimum; the base zone's solid profile keeps its pinned 0.8 mm bottom
/// chamfer radius as the sharpest curve below the base top.
#[test]
fn no_circular_edge_above_the_base_is_below_1mm() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    let minimum = common::number(report, "body_6u.min_circle_radius_above_base_mm");
    assert!(
        minimum >= 1.0,
        "an internal corner above the base is sharper than 1.0 mm: {minimum} mm"
    );
    // The sharpest internal curve above the base is the pinned lip neck.
    common::assert_close(minimum, 1.15, "sharpest circular edge above the base");
    // ADR-0013: the hidden cavity is gone, so the base zone's sharpest curve
    // is the pinned profile's 0.8 mm bottom chamfer radius.
    common::assert_close(
        common::number(report, "body_6u.min_circle_radius_base_zone_mm"),
        0.8,
        "base zone profile minimum radius",
    );
}

/// The lip keeps the pinned profile radii: `OUTER_RADIUS - offset`.
#[test]
fn the_lip_rows_keep_outer_radius_minus_offset() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    let rows = common::value(report, "body_6u.lip_rows")
        .as_array()
        .expect("lip_rows is a list");
    let expected = [
        (-2.4, 2.0, 37.5, 1.75),
        (-1.8, 2.6, 36.3, 1.15),
        (0.0, 2.6, 36.3, 1.15),
        (0.7, 1.9, 37.7, 1.85),
        (2.5, 1.9, 37.7, 1.85),
    ];
    assert_eq!(rows.len(), expected.len(), "lip row count");
    for (row, (z_relative, offset, width, radius)) in rows.iter().zip(expected) {
        common::assert_close(
            row["z_relative_mm"].as_f64().expect("lip row z"),
            z_relative,
            "lip row height",
        );
        common::assert_close(
            row["expected_offset_mm"].as_f64().expect("lip row offset"),
            offset,
            "lip row pinned offset",
        );
        common::assert_close(
            row["measured"]["width_mm"]
                .as_f64()
                .expect("measured width"),
            width,
            "lip opening width",
        );
        common::assert_close_list(&row["measured"]["radii"], &[radius], "lip opening radius");
        common::assert_close(
            row["expected_radius_mm"].as_f64().expect("expected radius"),
            radius,
            "OUTER_RADIUS - offset",
        );
    }
}

/// The 1x1 3U boundary body is filleted the same way as the 6U default.
#[test]
fn the_3u_boundary_body_is_filleted_too() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(common::value(report, "body_3u.solids").as_u64(), Some(1));
    common::assert_near(
        common::number(report, "body_3u.height_mm"),
        25.4,
        0.2,
        "3U height",
    );
    common::assert_near(
        common::number(report, "body_3u.volume_mm3"),
        EXPECTED_VOLUME_3U_MM3,
        EXPECTED_VOLUME_3U_MM3 * 0.03,
        "3U body volume",
    );
    common::assert_close(
        common::number(report, "body_3u.floor_fillet_used_mm"),
        2.0,
        "3U floor fillet",
    );
    common::assert_close_list(
        common::value(report, "body_3u.sections.mid_body.0.radii"),
        &[1.75],
        "3U inner vertical corner radius",
    );
    common::assert_close_list(
        common::value(report, "body_3u.sections.mid_body.1.radii"),
        &[3.75],
        "3U outer vertical corner radius",
    );
    common::assert_close_list(
        common::value(report, "body_3u.cove_vertical_radii"),
        &[2.0],
        "3U floor cove radius",
    );
    assert!(
        common::number(report, "body_3u.min_circle_radius_above_base_mm") >= 1.0,
        "3U internal corner sharper than 1.0 mm: {}",
        common::number(report, "body_3u.min_circle_radius_above_base_mm")
    );
}
