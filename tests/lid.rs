//! SYS-2.0 inset friction lid contract (ticket #9, standard §4.4/§4.5).
//!
//! `cad/lid.py` builds the lid; `cad/tests/lid.py` measures it headless on a
//! non-square 2 x 4 x 6U footprint and on the 1 x 1 pins. This test asserts
//! the standard's numbers: 2.0 mm plate, 2.0 x 2.0 mm skirt, 0.30 mm per-side
//! clearance to the lip, 1.40 mm plate corners, 1.0 mm top roundover, 0.6 mm
//! skirt-bottom chamfer, the 20 mm finger hole 14 mm from the +y footprint
//! edge, the top-only lid rule and the seated-base clearance finding
//! (human checkpoint #14), and
//! the optional 0.15 mm friction ribs.
//!
//! Like `tests/body_radii.rs`, this skips on a native host without FreeCAD.
//! The Docker gate and CI set `BOXGEN_REQUIRE_FREECAD=1`, where a missing
//! `freecadcmd` or a drifting dimension is a hard failure.

mod common;

use std::sync::OnceLock;

use serde_json::Value;

/// Comparison tolerance for dimensions in millimetres.
const EPS: f64 = 1e-6;

/// Runs `cad/tests/lid.py` under `freecadcmd` once and parses its JSON.
fn report() -> &'static Value {
    static REPORT: OnceLock<Value> = OnceLock::new();

    REPORT.get_or_init(|| common::script_report("lid.py"))
}

/// Finds a circular edge by radius, centre, and (optionally) z span.
fn circle<'a>(
    report: &'a Value,
    prefix: &str,
    radius: f64,
    centre: Option<(f64, f64)>,
    z_span: Option<(f64, f64)>,
) -> &'a Value {
    common::value(report, &format!("{prefix}.circles"))
        .as_array()
        .unwrap_or_else(|| panic!("{prefix}.circles is not a list"))
        .iter()
        .find(|entry| {
            let entry_radius = entry["radius_mm"].as_f64().unwrap_or(f64::NAN);
            let centre_matches = centre.is_none_or(|(x, y)| {
                (entry["centre_x_mm"].as_f64().unwrap_or(f64::NAN) - x).abs() < EPS
                    && (entry["centre_y_mm"].as_f64().unwrap_or(f64::NAN) - y).abs() < EPS
            });
            let span_matches = z_span.is_none_or(|(z_min, z_max)| {
                (entry["z_min_mm"].as_f64().unwrap_or(f64::NAN) - z_min).abs() < EPS
                    && (entry["z_max_mm"].as_f64().unwrap_or(f64::NAN) - z_max).abs() < EPS
            });
            (entry_radius - radius).abs() < EPS && centre_matches && span_matches
        })
        .unwrap_or_else(|| {
            panic!(
                "{prefix}: no circular edge of radius {radius} mm, centre {centre:?}, z {z_span:?}"
            )
        })
}

/// The module's constants mirror the standard, never re-typed elsewhere.
#[test]
fn lid_constants_pin_the_standard() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert!(common::flag(report, "module_imported"));
    common::assert_close(common::number(report, "constants.LID_PLATE"), 2.0, "plate");
    common::assert_close(common::number(report, "constants.LID_SKIRT"), 2.0, "skirt");
    common::assert_close(
        common::number(report, "constants.LID_CLEARANCE"),
        0.30,
        "skirt clearance",
    );
    common::assert_close(
        common::number(report, "constants.LID_CORNER_RADIUS"),
        1.40,
        "plate corner radius",
    );
    common::assert_close(
        common::number(report, "constants.LID_SKIRT_CHAMFER"),
        0.6,
        "skirt bottom chamfer",
    );
    common::assert_close(
        common::number(report, "constants.LID_TOP_ROUNDOVER"),
        1.0,
        "top roundover",
    );
    common::assert_close(
        common::number(report, "constants.FINGER_HOLE_DIAMETER"),
        20.0,
        "finger hole diameter",
    );
    common::assert_close(
        common::number(report, "constants.FINGER_HOLE_EDGE_DISTANCE"),
        14.0,
        "finger hole edge distance",
    );
    common::assert_close(
        common::number(report, "constants.LID_RIB_PROUD"),
        0.15,
        "rib interference",
    );
    common::assert_close(
        common::number(report, "constants.LID_RIB_LENGTH"),
        10.0,
        "rib length",
    );
    common::assert_close(
        common::number(report, "constants.LID_RIB_HEIGHT"),
        1.5,
        "rib height",
    );
    common::assert_close(
        common::number(report, "constants.SKIRT_INTRUSION"),
        1.0,
        "skirt intrusion below the lip bottom",
    );
    common::assert_close(
        common::number(report, "constants.LIP_VERTICAL_OFFSET"),
        1.9,
        "lip vertical-section inner offset",
    );
    // The lip vertical offset must come from the pinned profile components,
    // not from a re-typed literal.
    common::assert_close(
        common::number(report, "constants.OUTER_RADIUS"),
        3.75,
        "outer radius",
    );
    common::assert_close(
        common::number(report, "constants.LIP_HEIGHT"),
        4.4,
        "lip height",
    );
}

/// The 2 x 4 x 6U lid is one valid closed solid with the standard's z range.
#[test]
fn the_non_square_lid_is_one_closed_solid_at_the_standard_height() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(common::number(report, "lid_plain.solids"), 1.0);
    assert_eq!(common::number(report, "lid_plain.shells"), 1.0);
    assert!(common::flag(report, "lid_plain.is_valid"));
    assert!(common::flag(report, "lid_plain.is_closed"));

    // The plate is 0.30 mm clear of the lip vertical section on both axes:
    // a square-lid mix-up would show up here.
    common::assert_close(
        common::number(report, "lid_plain.bbox.x_mm"),
        79.1,
        "lid bbox x (plate width)",
    );
    common::assert_close(
        common::number(report, "lid_plain.bbox.y_mm"),
        163.1,
        "lid bbox y (plate depth)",
    );
    common::assert_close(
        common::number(report, "lid_plain.expected.plate_x_mm"),
        79.1,
        "expected plate width",
    );
    common::assert_close(
        common::number(report, "lid_plain.expected.plate_y_mm"),
        163.1,
        "expected plate depth",
    );

    // Lid top 1.0 mm below the lip top, skirt 1.0 mm below the lip bottom.
    common::assert_close(
        common::number(report, "lid_plain.bbox.z_max_mm"),
        45.0,
        "lid top",
    );
    common::assert_close(
        common::number(report, "lid_plain.bbox.z_min_mm"),
        41.0,
        "skirt bottom",
    );
    common::assert_close(
        common::number(report, "lid_plain.expected.z_top_mm"),
        45.0,
        "z_top = lip bottom + 3.0",
    );

    let volume = common::number(report, "lid_plain.volume_mm3");
    assert!(
        (volume - 26_785.19).abs() <= 26_785.19 * 0.01,
        "lid volume drifted: {volume} mm3"
    );
    assert!(common::number(report, "lid_plain.area_mm2") > 0.0);
}

/// Plate 2.0 thick, skirt 2.0 thick: slices above the plate bottom are solid
/// plate (with the finger hole), slices below are the skirt ring only.
#[test]
fn the_plate_and_skirt_are_2mm() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    // Just above the plate bottom: plate with the 20 mm hole (2 wires).
    assert_eq!(
        common::list_len(report, "lid_plain.sections.plate_above_bottom.wires"),
        2
    );
    common::assert_close(
        common::number(
            report,
            "lid_plain.sections.plate_above_bottom.wires.0.width_mm",
        ),
        20.0,
        "finger hole at the plate bottom",
    );
    common::assert_close(
        common::number(
            report,
            "lid_plain.sections.plate_above_bottom.wires.1.width_mm",
        ),
        79.1,
        "plate width above the plate bottom",
    );

    // Mid plate: still the full plate (plate thickness 2.0).
    common::assert_close(
        common::number(report, "lid_plain.sections.plate_mid.width_mm"),
        79.1,
        "plate width at mid thickness",
    );
    common::assert_close(
        common::number(report, "lid_plain.sections.plate_mid.depth_mm"),
        163.1,
        "plate depth at mid thickness",
    );

    // Just below the plate bottom: the skirt ring only (outer + inner wire).
    assert_eq!(
        common::list_len(report, "lid_plain.sections.below_plate.wires"),
        2
    );
    let outer = common::number(report, "lid_plain.sections.below_plate.wires.1.width_mm");
    let inner = common::number(report, "lid_plain.sections.below_plate.wires.0.width_mm");
    common::assert_close(outer - inner, 4.0, "skirt thickness at the collar");

    // Through the skirt pilot: 2.0 mm thick there too.
    let pilot_outer = common::number(report, "lid_plain.sections.pilot.wires.1.width_mm");
    let pilot_inner = common::number(report, "lid_plain.sections.pilot.wires.0.width_mm");
    common::assert_close(pilot_outer, 77.7, "pilot outer width");
    common::assert_close(
        pilot_outer - pilot_inner,
        4.0,
        "skirt thickness at the pilot",
    );
}

/// 0.30 mm clearance per side to the lip: neck and vertical section.
#[test]
fn the_skirt_and_plate_clear_the_lip_by_030_per_side() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    // The pilot sits inside the lip neck (offset 2.6).
    let neck_opening = common::number(report, "body.neck.wires.0.width_mm");
    let pilot = common::number(report, "lid_plain.sections.pilot.wires.1.width_mm");
    common::assert_close(neck_opening, 78.3, "lip neck opening width");
    common::assert_close(
        (neck_opening - pilot) / 2.0,
        0.30,
        "pilot clearance to the lip neck per side",
    );

    // The plate sits inside the lip vertical section (offset 1.9).
    let vertical_opening = common::number(report, "body.vertical_section.wires.0.width_mm");
    common::assert_close(vertical_opening, 79.7, "lip vertical opening width");
    common::assert_close(
        (vertical_opening - common::number(report, "lid_plain.bbox.x_mm")) / 2.0,
        0.30,
        "plate clearance to the lip vertical section per side",
    );

    // Same on the y axis.
    common::assert_close(
        (common::number(report, "body.vertical_section.wires.0.depth_mm")
            - common::number(report, "lid_plain.bbox.y_mm"))
            / 2.0,
        0.30,
        "plate clearance on y",
    );
}

/// 1.40 mm plate corners, 1.0 mm top roundover, 0.6 mm skirt chamfer.
#[test]
fn the_lid_edges_follow_the_standard() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    // Plate corner cylinders: radius 1.40, trimmed by the roundover at 44.0,
    // so the corner wall runs from the plate bottom (43.0) to 44.0.
    circle(
        report,
        "lid_plain",
        1.40,
        Some((-38.15, -80.15)),
        Some((43.0, 43.0)),
    );
    circle(
        report,
        "lid_plain",
        1.40,
        Some((-38.15, -80.15)),
        Some((44.0, 44.0)),
    );

    // 1.0 mm roundover on the plate top: the applied band spans 44.0..45.0 and
    // the top face is inset by the radius.
    assert!(common::flag(report, "lid_plain.roundover_applied"));
    common::assert_close(
        common::number(report, "lid_plain.top_face_width_mm"),
        79.1 - 2.0,
        "top face width (plate - 2 * roundover)",
    );
    let roundover = circle(
        report,
        "lid_plain",
        1.0,
        Some((-38.55, -80.15)),
        Some((44.0, 45.0)),
    );
    common::assert_close(
        roundover["z_min_mm"].as_f64().unwrap(),
        44.0,
        "roundover z min",
    );
    common::assert_close(
        roundover["z_max_mm"].as_f64().unwrap(),
        45.0,
        "roundover z max",
    );

    // 0.6 mm skirt bottom chamfer: the outer face recedes 0.55 per side at
    // 0.05 above the skirt bottom and 0.05 per side at 0.55.
    let pilot = common::number(report, "lid_plain.sections.pilot.wires.1.width_mm");
    common::assert_near(
        pilot - common::number(report, "lid_plain.sections.chamfer_low.width_mm"),
        2.0 * 0.55,
        0.05,
        "skirt chamfer recess at 0.05 above the bottom",
    );
    common::assert_near(
        pilot - common::number(report, "lid_plain.sections.chamfer_high.width_mm"),
        2.0 * 0.05,
        0.05,
        "skirt chamfer recess at 0.55 above the bottom",
    );
}

/// The 20 mm finger hole is centred 14 mm from the +y footprint edge.
#[test]
fn the_finger_hole_is_20mm_14mm_from_the_positive_y_edge() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    let centre_y = common::number(report, "lid_plain.expected.hole_centre_y_mm");
    common::assert_close(centre_y, 69.75, "hole centre y (outer_y/2 - 14)");

    // The hole pierces the 2.0 mm plate: rim circles at both plate faces.
    circle(
        report,
        "lid_plain",
        10.0,
        Some((0.0, centre_y)),
        Some((43.0, 43.0)),
    );
    circle(
        report,
        "lid_plain",
        10.0,
        Some((0.0, centre_y)),
        Some((45.0, 45.0)),
    );

    // The cut through the plate is exactly 20.0 wide at the plate bottom.
    common::assert_close(
        common::number(
            report,
            "lid_plain.sections.plate_above_bottom.wires.0.width_mm",
        ),
        20.0,
        "finger hole diameter",
    );
    // And it is centred: the plate slice stays symmetric.
    common::assert_close(
        common::number(report, "lid_plain.sections.plate_above_bottom.width_mm"),
        79.1,
        "plate slice symmetry",
    );
}

/// SYS-2.0 lids are top-only. The body's nominal top plane is `U * 7`
/// (42.0 mm for 6U), the official 4.4 mm lip sits above it, and the seated
/// base's physical pitch is `U * 7 - 0.35` (41.65 mm). The lid top is
/// 3.0 mm above the lip bottom (45.0 mm), so a lid left under another body
/// would be hit by the base; that is why the standard forbids it.
#[test]
fn lids_are_top_only_and_out_of_the_stack_load_path() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    let lid_top = common::number(report, "lid_plain.bbox.z_max_mm");
    let pitch = common::number(report, "stack.pitch_mm");
    common::assert_close(
        common::number(report, "stack.body_height_mm"),
        46.4,
        "overall 6U body height",
    );
    common::assert_close(
        common::number(report, "stack.body_top_mm"),
        42.0,
        "body top / lip bottom",
    );
    common::assert_close(pitch, 41.65, "physical stack pitch for 6U");
    common::assert_close(lid_top, 45.0, "lid top");
    common::assert_close(lid_top - pitch, 3.35, "seated base underside to lid top");

    // The measured lip contact seat: the base's bottom chamfer rests on the
    // lip's upper ramp with the base bottom 0.35 mm below the lip bottom.
    // The lip intersection is zero at the seat and positive just below it.
    common::assert_near(
        common::number(report, "stack.seat_rel_lip_bottom_mm"),
        -0.35,
        0.01,
        "base seat relative to the lip bottom",
    );
    assert!(
        common::number(report, "stack.seat_intersection_mm3").abs() < 1e-3,
        "the seat must not intersect the lip: {} mm3",
        common::number(report, "stack.seat_intersection_mm3")
    );
    assert!(
        common::number(report, "stack.seat_contact_below_mm3") > 0.0,
        "the seat must be a real contact: {} mm3",
        common::number(report, "stack.seat_contact_below_mm3")
    );

    // Checkpoint #14 finding: at the seat the seated base's solid underside
    // lies 3.35 mm below the lid's top face (a negative clearance), and the
    // base's wall material passes the lid's plate edge at the plate's mid
    // height. With the lid installed the base cannot reach the lip seat, so
    // the lid -- not the lip -- would carry a stacked body.
    assert!(
        common::number(report, "stack.base_lid_intersection_mm3") > 0.0,
        "expected the seated base to interfere with the lid: {} mm3",
        common::number(report, "stack.base_lid_intersection_mm3")
    );
    common::assert_near(
        common::number(report, "stack.base_underside_z_mm"),
        41.65,
        0.02,
        "seated base underside at the cell centre",
    );
    common::assert_near(
        common::number(report, "stack.lid_top_clearance_mm"),
        -3.35,
        0.02,
        "base underside to lid top clearance (negative at the seat)",
    );
    common::assert_near(
        common::number(report, "stack.base_wall_radius_mm"),
        18.6,
        0.02,
        "seated base wall radius at the plate's mid height",
    );
    common::assert_near(
        common::number(report, "stack.plate_edge_radius_mm"),
        18.55,
        0.02,
        "lid plate edge radius measured from the cell centre",
    );
    assert!(
        common::number(report, "stack.plate_edge_overlap_mm") > 0.0,
        "the base's solid wall must pass the plate edge: {} mm",
        common::number(report, "stack.plate_edge_overlap_mm")
    );

    // Nothing protrudes beyond the footprint: the lid stays inside the body.
    assert!(
        common::number(report, "lid_plain.bbox.x_mm")
            < common::number(report, "body.neck.width_mm"),
        "lid must not exceed the body footprint"
    );
}

/// `ribs=True` adds exactly two 0.15 mm x 10 mm ribs, inside the footprint;
/// the plain lid has none.
#[test]
fn the_optional_ribs_are_two_015mm_protrusions_on_the_skirt() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    // The plain lid has no ribs at all: the pilot is the expected width.
    common::assert_close(
        common::number(report, "lid_plain.sections.pilot.wires.1.width_mm"),
        common::number(report, "lid_plain.expected.pilot_x_mm"),
        "plain pilot width",
    );

    // Two ribs, one per side: the pilot slice grows by exactly 0.30 mm.
    common::assert_close(
        common::number(report, "lid_ribs.sections.pilot.wires.1.width_mm")
            - common::number(report, "lid_plain.sections.pilot.wires.1.width_mm"),
        2.0 * 0.15,
        "ribs protrude 0.15 per side",
    );

    // ... and by exactly two 10 mm x 0.15 mm cross-sections of area.
    common::assert_close(
        common::number(report, "lid_ribs.sections.pilot.area_mm2")
            - common::number(report, "lid_plain.sections.pilot.area_mm2"),
        2.0 * 10.0 * 0.15,
        "rib cross-section area (exactly two ribs)",
    );

    // The ribs stay inside the plate footprint (nothing protrudes).
    common::assert_close(
        common::number(report, "lid_ribs.bbox.x_mm"),
        common::number(report, "lid_plain.bbox.x_mm"),
        "ribbed bbox x",
    );
    common::assert_close(
        common::number(report, "lid_ribs.bbox.y_mm"),
        common::number(report, "lid_plain.bbox.y_mm"),
        "ribbed bbox y",
    );
    common::assert_close(
        common::number(report, "lid_ribs.bbox.z_max_mm"),
        common::number(report, "lid_plain.bbox.z_max_mm"),
        "ribbed bbox z",
    );

    // Still one valid closed solid, and the visible rib is small.
    assert_eq!(common::number(report, "lid_ribs.solids"), 1.0);
    assert!(common::flag(report, "lid_ribs.is_closed"));
    let added = common::number(report, "lid_ribs.volume_mm3")
        - common::number(report, "lid_plain.volume_mm3");
    assert!(
        added > 0.0 && added < 2.0 * 10.0 * 1.5 * 0.15,
        "rib volume {added} mm3 must be positive and below the raw box volume"
    );
    assert_eq!(
        common::number(report, "lid_ribs.faces") - common::number(report, "lid_plain.faces"),
        8.0,
        "two box ribs add four visible faces each",
    );
}

/// The square 1 x 1 lids stay at the prototype's dimensions (parity pin).
#[test]
fn the_1x1_lids_match_the_prototype() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(common::number(report, "lid_1x1.solids"), 1.0);
    common::assert_close(
        common::number(report, "lid_1x1.bbox.x_mm"),
        37.1,
        "1x1 lid width",
    );
    common::assert_close(
        common::number(report, "lid_1x1.bbox.y_mm"),
        37.1,
        "1x1 lid depth",
    );
    common::assert_close(
        common::number(report, "lid_1x1.bbox.z_min_mm"),
        41.0,
        "1x1 lid bottom",
    );
    common::assert_close(
        common::number(report, "lid_1x1.bbox.z_max_mm"),
        45.0,
        "1x1 lid top",
    );
    let volume = common::number(report, "lid_1x1.volume_mm3");
    assert!(
        (volume - 2_584.66).abs() <= 2_584.66 * 0.02,
        "1x1 lid volume drifted from the prototype: {volume} mm3"
    );

    // The 3U lid: 20.0 .. 24.0 (25.4 total height, lid top 1.4 below the top).
    assert_eq!(common::number(report, "lid_1x1_3u.solids"), 1.0);
    common::assert_close(
        common::number(report, "lid_1x1_3u.bbox.z_min_mm"),
        20.0,
        "3U lid bottom",
    );
    common::assert_close(
        common::number(report, "lid_1x1_3u.bbox.z_max_mm"),
        24.0,
        "3U lid top",
    );
}
