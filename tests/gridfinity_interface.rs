//! Pinned Gridfinity interface contract.
//!
//! `cad/gridfinity.py` vendors the base and stacking-lip profiles from the
//! FreeCAD Gridfinity Workbench (LGPL-2.1). Those profiles are the interface
//! between our bodies, community baseplates, and the ecosystem, so this test
//! builds them headless and measures the result instead of trusting the
//! constants.
//!
//! Like `tests/freecad_env.rs`, the test skips on a native host without
//! FreeCAD. The Docker gate and CI set `BOXGEN_REQUIRE_FREECAD=1`, where a
//! missing `freecadcmd` or a drifting profile is a hard failure.

mod common;

use std::sync::OnceLock;

use serde_json::Value;

/// Runs `cad/tests/interface.py` under `freecadcmd` once and parses its JSON.
fn report() -> &'static Value {
    static REPORT: OnceLock<Value> = OnceLock::new();

    REPORT.get_or_init(|| common::script_report("interface.py"))
}

/// Asserts the base and lip segment splits add up to the pinned heights.
#[test]
fn constants_pin_the_gridfinity_grid_and_profile_heights() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(
        common::value(report, "module_imported").as_bool(),
        Some(true),
        "cad/gridfinity.py did not import"
    );

    common::assert_close(common::number(report, "constants.GRID"), 42.0, "grid pitch");
    common::assert_close(
        common::number(report, "constants.CLEARANCE"),
        0.25,
        "per-side clearance",
    );
    common::assert_close(
        common::number(report, "constants.NOMINAL_WIDTH_1U"),
        41.5,
        "nominal 1-unit outer width",
    );
    common::assert_close(
        common::number(report, "outer_width.1"),
        41.5,
        "outer_width(1)",
    );
    common::assert_close(
        common::number(report, "outer_width.2"),
        83.5,
        "outer_width(2)",
    );
    common::assert_close(
        common::number(report, "outer_width.4"),
        167.5,
        "outer_width(4)",
    );

    common::assert_close(
        common::number(report, "constants.BASE_HEIGHT"),
        4.75,
        "base profile height",
    );
    let base_split = common::number(report, "constants.BASE_BOTTOM_CHAMFER")
        + common::number(report, "constants.BASE_VERTICAL")
        + common::number(report, "constants.BASE_TOP_CHAMFER");
    common::assert_close(base_split, 4.75, "base 0.8 + 1.8 + 2.15 split");
    common::assert_close(
        common::number(report, "constants.BASE_BOTTOM_CHAMFER"),
        0.8,
        "base bottom chamfer",
    );
    common::assert_close(
        common::number(report, "constants.BASE_VERTICAL"),
        1.8,
        "base vertical section",
    );
    common::assert_close(
        common::number(report, "constants.BASE_TOP_CHAMFER"),
        2.15,
        "base top chamfer",
    );
    common::assert_close(
        common::number(report, "constants.BASE_BOTTOM_RADIUS"),
        0.8,
        "base bottom corner radius",
    );
    common::assert_close(
        common::number(report, "constants.BASE_VERTICAL_RADIUS"),
        1.6,
        "base vertical-section corner radius",
    );
    common::assert_close(
        common::number(report, "constants.OUTER_RADIUS"),
        3.75,
        "outer corner radius",
    );
    common::assert_close(
        common::number(report, "constants.BASE_SHELL"),
        1.2,
        "base zone side shell",
    );

    common::assert_close(
        common::number(report, "constants.LIP_HEIGHT"),
        4.0,
        "stacking lip height",
    );
    let lip_split = common::number(report, "constants.LIP_BOTTOM_CHAMFER")
        + common::number(report, "constants.LIP_VERTICAL")
        + common::number(report, "constants.LIP_TOP_CHAMFER");
    common::assert_close(lip_split, 4.0, "lip 0.7 + 1.8 + 1.5 split");
    common::assert_close(
        common::number(report, "constants.LIP_BOTTOM_CHAMFER"),
        0.7,
        "lip bottom chamfer",
    );
    common::assert_close(
        common::number(report, "constants.LIP_VERTICAL"),
        1.8,
        "lip vertical section",
    );
    common::assert_close(
        common::number(report, "constants.LIP_TOP_CHAMFER"),
        1.5,
        "lip top chamfer",
    );
    common::assert_close(
        common::number(report, "constants.LIP_TOP_LEDGE"),
        0.4,
        "lip top ledge",
    );
    common::assert_close(
        common::number(report, "constants.LIP_WALL"),
        2.0,
        "lip wall",
    );
    common::assert_close(
        common::number(report, "constants.LIP_OVERLAP"),
        2.4,
        "lip skirt overlap below the body top",
    );
}

/// The module must not import or require the installed workbench.
#[test]
fn module_imports_without_the_gridfinity_workbench() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(
        common::value(report, "module_imported").as_bool(),
        Some(true),
        "cad/gridfinity.py did not import"
    );
    let loaded = common::value(report, "loaded_workbench_modules")
        .as_array()
        .expect("loaded_workbench_modules is a list");
    assert!(
        loaded.is_empty(),
        "vendored module loaded the installed workbench: {loaded:?}"
    );
}

/// The base foot lofts 35.6 -> 37.2 -> 37.2 -> 41.5 with the pinned radii.
#[test]
fn base_foot_matches_the_pinned_profile() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    common::assert_close(
        common::number(report, "base_foot.bbox.width"),
        41.5,
        "foot width",
    );
    common::assert_close(
        common::number(report, "base_foot.bbox.depth"),
        41.5,
        "foot depth",
    );
    common::assert_close(
        common::number(report, "base_foot.bbox.height"),
        4.75,
        "foot height",
    );

    let sections = common::value(report, "base_foot.sections")
        .as_array()
        .expect("base_foot.sections is a list");
    let expected = [
        (0.0, 35.6, &[0.8]),
        (0.8, 37.2, &[1.6]),
        (2.6, 37.2, &[1.6]),
        (4.75, 41.5, &[3.75]),
    ];
    assert_eq!(sections.len(), expected.len(), "base foot profile levels");
    for (section, (z, width, radii)) in sections.iter().zip(expected) {
        common::assert_close(
            section["z"].as_f64().expect("section z"),
            z,
            "base section height",
        );
        common::assert_close(
            section["width"].as_f64().expect("section width"),
            width,
            "base section width",
        );
        common::assert_close(
            section["depth"].as_f64().expect("section depth"),
            width,
            "base section depth",
        );
        common::assert_close_list(&section["radii"], radii, "base section corner radii");
    }

    // The underside cavity leaves the standard's 1.2 mm base shell; the
    // 4x4 footprint then covers 16 cells on a 42 mm pitch.
    let shell = (common::number(report, "base_foot.sections.1.width")
        - common::number(report, "base_under_cutter.sections.0.width"))
        / 2.0;
    common::assert_close(
        shell,
        common::number(report, "constants.BASE_SHELL"),
        "base zone side shell",
    );
    common::assert_close(
        common::number(report, "base_under_cutter.sections.0.depth"),
        34.8,
        "base cavity depth",
    );
    common::assert_close_list(
        common::value(report, "base_under_cutter.sections.0.radii"),
        &[0.4],
        "base cavity minimum corner radius",
    );

    common::assert_close(
        common::number(report, "base_feet.2x1.width"),
        83.5,
        "2x1 base width",
    );
    common::assert_close(
        common::number(report, "base_feet.2x1.depth"),
        41.5,
        "2x1 base depth",
    );
    assert_eq!(
        common::value(report, "base_feet.2x1.solids").as_u64(),
        Some(2),
        "2x1 base has one foot per cell"
    );
    common::assert_close(
        common::number(report, "base_feet.4x4.width"),
        167.5,
        "4x4 base width",
    );
    common::assert_close(
        common::number(report, "base_feet.4x4.depth"),
        167.5,
        "4x4 base depth",
    );
    common::assert_close(
        common::number(report, "base_feet.4x4.height"),
        4.75,
        "4x4 base height",
    );
    assert_eq!(
        common::value(report, "base_feet.4x4.solids").as_u64(),
        Some(16),
        "4x4 base has one foot per cell"
    );
}

/// The lip profile is pinned point-for-point, then measured on the solid.
#[test]
fn stacking_lip_matches_the_pinned_profile() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    let profile = common::value(report, "lip.profile")
        .as_array()
        .expect("lip.profile is a list");
    let expected = [
        (0.25, 0.0),
        (0.25, 4.0),
        (0.65, 4.0),
        (2.15, 2.5),
        (2.15, 0.7),
        (2.85, 0.0),
        (2.85, -1.8),
        (2.25, -2.4),
        (0.25, -2.4),
    ];
    assert_eq!(profile.len(), expected.len(), "lip profile point count");
    for (point, (x, z)) in profile.iter().zip(expected) {
        let point = point.as_array().expect("profile point is a pair");
        common::assert_close(point[0].as_f64().expect("profile x"), x, "lip profile x");
        common::assert_close(point[1].as_f64().expect("profile z"), z, "lip profile z");
    }

    let offsets = common::value(report, "lip.offsets")
        .as_array()
        .expect("lip.offsets is a list");
    let expected = [
        (-2.4, 2.0),
        (-1.8, 2.6),
        (0.0, 2.6),
        (0.7, 1.9),
        (2.5, 1.9),
        (4.0, 0.4),
    ];
    assert_eq!(offsets.len(), expected.len(), "lip offset rows");
    for (row, (z, offset)) in offsets.iter().zip(expected) {
        common::assert_close(row["z"].as_f64().expect("row z"), z, "lip offset row z");
        common::assert_close(
            row["offset"].as_f64().expect("row offset"),
            offset,
            "lip inner offset",
        );
    }

    // The cutter is the void #6 removes; its width is the inner opening.
    let sections = common::value(report, "lip.cut_sections")
        .as_array()
        .expect("lip.cut_sections is a list");
    let expected = [
        (-2.4, 37.5, &[1.75][..]),
        (-1.8, 36.3, &[1.15][..]),
        (0.0, 36.3, &[1.15][..]),
        (0.7, 37.7, &[1.85][..]),
        (2.5, 37.7, &[1.85][..]),
        (4.0, 40.7, &[3.35][..]),
    ];
    assert_eq!(sections.len(), expected.len(), "lip cutter levels");
    for (section, (z, width, radii)) in sections.iter().zip(expected) {
        common::assert_close(
            section["z"].as_f64().expect("section z"),
            z,
            "lip section height",
        );
        common::assert_close(
            section["width"].as_f64().expect("lip opening width"),
            width,
            "lip inner opening width",
        );
        common::assert_close(
            section["depth"].as_f64().expect("lip opening depth"),
            width,
            "lip inner opening depth",
        );
        common::assert_close_list(&section["radii"], radii, "lip inner corner radius");
    }

    // Stacking fit: the 37.2 mm base vertical section drops into the 37.7 mm
    // lip vertical opening with 0.25 mm per side.
    let base_vertical = common::number(report, "base_foot.sections.1.width");
    let lip_vertical = common::number(report, "lip.cut_sections.3.width");
    common::assert_close(
        lip_vertical - base_vertical,
        2.0 * common::number(report, "constants.CLEARANCE"),
        "base-to-lip clearance per side",
    );

    // Ring: flush with the body outer face, 4.0 mm above the lip bottom,
    // 2.4 mm of skirt below it, and the workbench's swept volume.
    common::assert_close(
        common::number(report, "lip.ring.bbox.width"),
        41.5,
        "ring outer width",
    );
    common::assert_close(
        common::number(report, "lip.ring.bbox.depth"),
        41.5,
        "ring outer depth",
    );
    common::assert_close(
        common::number(report, "lip.ring.height_above_bottom"),
        4.0,
        "lip height above the body top",
    );
    common::assert_close(
        common::number(report, "lip.ring.overlap_below_bottom"),
        2.4,
        "lip overlap below the body top",
    );
    common::assert_close(
        common::number(report, "lip.ring.bbox.zmin"),
        -2.4,
        "ring bottom",
    );
    common::assert_close(
        common::number(report, "lip.ring.bbox.zmax"),
        4.0,
        "ring top",
    );
    assert!(
        (common::number(report, "lip.ring.volume") - 1_952.194_200_107_862_6).abs() < 1e-3,
        "lip volume drifted from the workbench reference: {} mm^3",
        common::number(report, "lip.ring.volume")
    );

    // Rectangular footprints use the same profile on both axes.
    common::assert_close(
        common::number(report, "lip.rect_ring.bbox.width"),
        83.5,
        "2x1 lip outer width",
    );
    common::assert_close(
        common::number(report, "lip.rect_ring.bbox.depth"),
        41.5,
        "2x1 lip outer depth",
    );
    common::assert_close(
        common::number(report, "lip.rect_ring.bbox.zmin"),
        -2.4,
        "2x1 lip bottom",
    );
    common::assert_close(
        common::number(report, "lip.rect_ring.bbox.zmax"),
        4.0,
        "2x1 lip top",
    );
}
