//! The `/api/standard` mirror of the Python master (ADR-0002).
//!
//! `cad/tests/standard_mirror.py` imports the master modules and emits every
//! constant the Rust side mirrors. This test asserts each `src/tree.rs`
//! constant still equals its Python counterpart, so the serve panel and the
//! geometry the generated scripts build cannot drift apart silently; a
//! mirrored constant changes only as a deliberate spec change.
//!
//! Like the other geometry tests, this skips on a native host without
//! FreeCAD. The Docker gate sets `BOXGEN_REQUIRE_FREECAD=1`, where a drift is
//! a hard failure.

mod common;

use std::sync::OnceLock;

use serde_json::Value;

/// Runs `cad/tests/standard_mirror.py` once and parses its JSON.
fn report() -> &'static Value {
    static REPORT: OnceLock<Value> = OnceLock::new();

    REPORT.get_or_init(|| common::script_report("standard_mirror.py"))
}

/// Asserts one mirrored constant against its Rust tree constant.
fn assert_mirror(path: &str, expected: f64) {
    common::assert_close(common::number(report(), path), expected, path);
}

#[test]
fn the_rust_constants_and_the_python_master_agree() {
    use boxgen::tree;

    if !common::freecad_required() {
        return;
    }

    assert_mirror("gridfinity.grid_mm", tree::GRID_MM);
    assert_mirror(
        "gridfinity.clearance_per_side_mm",
        tree::OUTER_CLEARANCE_MM / 2.0,
    );
    assert_mirror("gridfinity.base_height_mm", tree::BASE_HEIGHT_MM);
    assert_mirror("gridfinity.lip_height_mm", tree::LIP_HEIGHT_MM);
    assert_mirror("gridfinity.outer_radius_mm", tree::OUTER_RADIUS_MM);

    assert_mirror("shell.wall_mm", tree::WALL_MM);
    assert_mirror("shell.height_unit_mm", tree::HEIGHT_UNIT_MM);
    assert_mirror("shell.floor_mm", tree::FLOOR_MM);
    assert_mirror("shell.floor_top_mm", tree::FLOOR_TOP_MM);
    assert_mirror("shell.inner_radius_mm", tree::INNER_RADIUS_MM);
    assert_mirror("shell.floor_fillet_mm", tree::FLOOR_FILLET_MM);

    assert_mirror("dividers.thickness_mm", tree::DIVIDER_MM);
    assert_mirror("dividers.top_chamfer_mm", tree::DIVIDER_TOP_CHAMFER_MM);
    assert_mirror("dividers.gap_3u_mm", tree::DIVIDER_GAP_3U_MM);
    assert_mirror("dividers.gap_mm", tree::DIVIDER_GAP_MM);
    assert_mirror("dividers.junction_fillet_mm", tree::DIVIDER_COVE_MM);

    assert_mirror("lid.plate_mm", tree::LID_PLATE_MM);
    assert_mirror("lid.skirt_mm", tree::LID_SKIRT_MM);
    assert_mirror("lid.clearance_per_side_mm", tree::LID_CLEARANCE_MM);
    assert_mirror("lid.corner_radius_mm", tree::LID_CORNER_RADIUS_MM);
    assert_mirror("lid.top_roundover_mm", tree::LID_TOP_ROUNDOVER_MM);
    assert_mirror("lid.skirt_chamfer_mm", tree::LID_SKIRT_CHAMFER_MM);
    assert_mirror(
        "lid.finger_hole_diameter_mm",
        tree::LID_FINGER_HOLE_DIAMETER_MM,
    );
    assert_mirror(
        "lid.finger_hole_edge_distance_mm",
        tree::LID_FINGER_HOLE_EDGE_DISTANCE_MM,
    );
    assert_mirror("lid.rib_protrusion_mm", tree::LID_RIB_MM);

    assert_mirror("estimate.density_g_per_cm3", tree::PETG_DENSITY_G_PER_CM3);
    assert_mirror(
        "estimate.effective_flow_mm3_per_s",
        tree::EFFECTIVE_FLOW_MM3_PER_S,
    );
}
