//! SYS-1.0 division tree divider contract (ticket #8, standard section 4.3).
//!
//! `cad/dividers.py` resolves a division tree to absolute divider rectangles
//! and fuses them into the body shell; `cad/tests/dividers.py` measures the
//! plan, the contract's golden 4 x 4 tree (x at 80; left child y at 50; right
//! child x at 40), every legal divider height, the floor and wall junction
//! coves, the 0.4 mm top chamfer, and two synthetic trees that force the
//! divider/junction onto the inner corner arc. This test asserts the pinned
//! numbers: 2.0 mm thickness, whole-parent spans, near-face positions on the
//! integer millimetre, tops at 15.0 / 28.0 / 49.0 / 70.0 mm, and a single
//! valid closed solid.
//!
//! Like `tests/body_radii.rs`, this skips on a native host without FreeCAD.
//! The Docker gate and CI set `BOXGEN_REQUIRE_FREECAD=1`, where a missing
//! `freecadcmd` or a drifting divider is a hard failure.

mod common;

use std::sync::OnceLock;

use serde_json::Value;

/// Comparison tolerance for exact millimetre values (slice faces).
const EPS: f64 = 1e-6;

/// Cove insets come from `Shape.slice`, which carries the kernel's section
/// tolerance; 0.005 mm is far below print tolerance but catches a wrong
/// radius (the 1.0 mm fallback would be 1.03 mm off).
const COVE_TOLERANCE: f64 = 0.005;

/// The divider tops are found by bisection with a 0.01 mm probe.
const TOP_TOLERANCE: f64 = 0.02;

/// Runs `cad/tests/dividers.py` under `freecadcmd` once and parses its JSON.
fn report() -> &'static Value {
    static REPORT: OnceLock<Value> = OnceLock::new();

    REPORT.get_or_init(|| common::script_report("dividers.py"))
}

/// Reads a wire bounding box as `[x_min, y_min, x_max, y_max]`.
fn wire_box(path: &str) -> [f64; 4] {
    let list = common::value(report(), path)
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

/// Asserts that the wire boxes at `path` are exactly `expected`, in any order.
fn assert_wire_boxes(expected: &[[f64; 4]], path: &str, tolerance: f64, what: &str) {
    let boxes = common::value(report(), path)
        .as_array()
        .unwrap_or_else(|| panic!("{path}: not a list of wire boxes"));
    assert_eq!(
        boxes.len(),
        expected.len(),
        "{what}: expected {} wires, measured {}: {boxes:?}",
        expected.len(),
        boxes.len()
    );

    let mut unmatched: Vec<[f64; 4]> = expected.to_vec();
    for (index, _) in boxes.iter().enumerate() {
        let actual = wire_box(&format!("{path}.{index}"));
        let Some(position) = unmatched.iter().position(|want| {
            want.iter()
                .zip(actual.iter())
                .all(|(want, got)| (want - got).abs() <= tolerance)
        }) else {
            panic!("{what}: wire {actual:?} matches no expected box in {expected:?}");
        };
        unmatched.remove(position);
    }
}

/// Finds the first wire box at `path` whose bounding box matches `predicate`.
fn wire_box_matching(path: &str, predicate: impl Fn(&[f64; 4]) -> bool) -> [f64; 4] {
    let boxes = common::value(report(), path)
        .as_array()
        .unwrap_or_else(|| panic!("{path}: not a list of wire boxes"));
    for index in 0..boxes.len() {
        let candidate = wire_box(&format!("{path}.{index}"));
        if predicate(&candidate) {
            return candidate;
        }
    }
    panic!("{path}: no wire box matches: {boxes:?}");
}

/// Asserts one cove circle with the pinned centre and radius.
fn assert_cove_circle(circles: &[Value], centre: [f64; 2], radius: f64, what: &str) {
    let found = circles.iter().any(|circle| {
        let measured_centre = [
            circle["centre_mm"][0].as_f64().expect("centre x"),
            circle["centre_mm"][1].as_f64().expect("centre y"),
        ];
        let measured_radius = circle["radius_mm"].as_f64().expect("radius");
        (measured_centre[0] - centre[0]).abs() < COVE_TOLERANCE
            && (measured_centre[1] - centre[1]).abs() < COVE_TOLERANCE
            && (measured_radius - radius).abs() < EPS
    });
    assert!(
        found,
        "{what}: no cove circle at {centre:?} r={radius} in {circles:?}"
    );
}

/// The master's constants mirror the standard.
#[test]
fn divider_constants_pin_the_standard() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(
        common::value(report, "module_imported").as_bool(),
        Some(true)
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
        common::number(report, "constants.DIVIDER_GAP_3U"),
        2.0,
        "3U divider gap",
    );
    common::assert_close(
        common::number(report, "constants.DIVIDER_GAP"),
        10.0,
        "6U-and-taller divider gap",
    );
    common::assert_close(
        common::number(report, "constants.JUNCTION_FILLET"),
        2.0,
        "junction cove radius",
    );
}

/// The plan resolves the contract's golden tree to absolute rectangles:
/// x at 80 lands on x = -1.75, the left child's y at 50 on y = -31.75, and
/// the right child's x at 40 on x = 40.25, each with the parent's full span.
#[test]
fn the_plan_resolves_the_golden_tree_to_absolute_rectangles() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(common::list_len(report, "plan_golden"), 3);

    let dividers = common::value(report, "plan_golden")
        .as_array()
        .expect("plan");
    let by_key = |axis: &str, near_face: f64| -> &Value {
        dividers
            .iter()
            .find(|divider| {
                divider["axis"].as_str() == Some(axis)
                    && (divider["near_face_mm"].as_f64().expect("near face") - near_face).abs()
                        < EPS
            })
            .unwrap_or_else(|| panic!("no {axis} divider at {near_face} in {dividers:?}"))
    };

    let root = by_key("x", -1.75);
    common::assert_close(
        root["span_lo_mm"].as_f64().expect("span"),
        -81.75,
        "root divider span low",
    );
    common::assert_close(
        root["span_hi_mm"].as_f64().expect("span"),
        81.75,
        "root divider span high",
    );
    assert_eq!(root["lo_end"].as_str(), Some("wall"));
    assert_eq!(root["hi_end"].as_str(), Some("wall"));

    let left = by_key("y", -31.75);
    common::assert_close(
        left["span_lo_mm"].as_f64().expect("span"),
        -81.75,
        "left divider span low",
    );
    common::assert_close(
        left["span_hi_mm"].as_f64().expect("span"),
        -1.75,
        "left divider span high",
    );
    assert_eq!(left["lo_end"].as_str(), Some("wall"));
    assert_eq!(left["hi_end"].as_str(), Some("tee"));

    let right = by_key("x", 40.25);
    common::assert_close(
        right["span_lo_mm"].as_f64().expect("span"),
        -81.75,
        "right divider span low",
    );
    common::assert_close(
        right["span_hi_mm"].as_f64().expect("span"),
        81.75,
        "right divider span high",
    );

    assert_eq!(common::list_len(report, "plan_corner"), 1);
    assert_eq!(common::list_len(report, "plan_single_leaf"), 0);
}

/// A leaf tree adds nothing; the single-compartment path is unchanged.
#[test]
fn a_leaf_tree_adds_no_material() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(common::value(report, "leaf_tree_solids").as_u64(), Some(1));
    common::assert_close(
        common::number(report, "leaf_tree_volume_mm3"),
        common::number(report, "plain_body_volume_mm3"),
        "leaf tree leaf volume",
    );
}

/// The golden tree is one valid, closed solid with a positive volume.
#[test]
fn the_golden_tree_is_one_valid_closed_solid() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(common::value(report, "golden.solids").as_u64(), Some(1));
    assert!(
        common::flag(report, "golden.is_valid"),
        "golden body is not valid"
    );
    assert!(
        common::flag(report, "golden.is_closed"),
        "golden body is not one closed shell"
    );
    assert!(
        common::number(report, "golden.volume_mm3") > 0.0,
        "golden body volume must be positive"
    );
    common::assert_close(
        common::number(report, "golden.divider_top_mm"),
        28.0,
        "6U divider top",
    );
}

/// Slicing at z = 20 (well below the 28.0 top) shows exactly the four
/// compartments the tree defines; their wire boxes pin the three dividers'
/// near faces and full spans.
#[test]
fn the_golden_slice_at_twenty_shows_the_three_dividers() {
    if !common::freecad_required() {
        return;
    }

    assert_wire_boxes(
        &[
            [-81.75, -81.75, -1.75, -31.75],
            [-81.75, -29.75, -1.75, 81.75],
            [0.25, -81.75, 40.25, 81.75],
            [42.25, -81.75, 81.75, 81.75],
        ],
        "golden.section_z20",
        EPS,
        "4 x 4 golden compartments at z = 20",
    );
}

/// Above the divider top the body is one open interior; at z = 30 no divider
/// material remains.
#[test]
fn the_golden_slice_at_thirty_shows_no_divider() {
    if !common::freecad_required() {
        return;
    }

    assert_wire_boxes(
        &[[-81.75, -81.75, 81.75, 81.75]],
        "golden.section_z30",
        EPS,
        "4 x 4 interior at z = 30",
    );
}

/// Below the chamfer band the compartments are flush with the divider faces;
/// 0.1 mm under the top the 0.4 mm chamfer has cut 0.3 mm into them.
#[test]
fn the_golden_divider_tops_carry_the_chamfer() {
    if !common::freecad_required() {
        return;
    }

    assert_wire_boxes(
        &[
            [-81.75, -81.75, -1.75, -31.75],
            [-81.75, -29.75, -1.75, 81.75],
            [0.25, -81.75, 40.25, 81.75],
            [42.25, -81.75, 81.75, 81.75],
        ],
        "golden.section_z27_5",
        EPS,
        "compartments 0.5 mm below the divider top",
    );

    assert_wire_boxes(
        &[
            [-81.75, -81.75, -1.45, -31.45],
            [-81.75, -30.05, -1.45, 81.75],
            [-0.05, -81.75, 40.55, 81.75],
            [41.95, -81.75, 81.75, 81.75],
        ],
        "golden.section_z27_9",
        COVE_TOLERANCE,
        "compartments 0.1 mm below the divider top (0.4 mm chamfer)",
    );
}

/// The 2.0 mm cove rails taper from the face: 1.032 mm clear 0.25 mm above
/// the floor, flush by 2.0 mm up, and gone above it.
#[test]
fn the_golden_floor_junctions_are_filleted() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    // Compartment A at z = 5.0: 1.0318 mm inset from every face.
    let inset = 2.0 - (4.0 - 1.75_f64.powi(2)).sqrt();
    let a = wire_box_matching("golden.section_z5_0", |box_| box_[1] < -60.0);
    common::assert_near(a[0], -81.75 + inset, COVE_TOLERANCE, "A cove inset (west)");
    common::assert_near(a[2], -1.75 - inset, COVE_TOLERANCE, "A cove inset (east)");
    common::assert_near(a[1], -81.75 + inset, COVE_TOLERANCE, "A cove inset (south)");
    common::assert_near(a[3], -31.75 - inset, COVE_TOLERANCE, "A cove inset (north)");

    // The cove is gone at z = 6.7 (2.0 mm above the floor): flush to the face.
    let b = wire_box_matching("golden.section_z6_7", |box_| box_[1] < -60.0);
    common::assert_near(
        b[0],
        -81.75,
        COVE_TOLERANCE,
        "west wall flush at the cove top",
    );
    common::assert_near(
        b[2],
        -1.75,
        COVE_TOLERANCE,
        "east face flush at the cove top",
    );

    // And clean above it.
    assert_wire_boxes(
        &[
            [-81.75, -81.75, -1.75, -31.75],
            [-81.75, -29.75, -1.75, 81.75],
            [0.25, -81.75, 40.25, 81.75],
            [42.25, -81.75, 81.75, 81.75],
        ],
        "golden.section_z7_5",
        EPS,
        "compartments 2.75 mm above the floor",
    );

    // Junction probes: material where the sharp corner used to be, void where
    // the cove does not reach (the T junction and the -y wall end).
    assert!(
        common::flag(report, "golden.tee_material"),
        "T junction not filleted"
    );
    assert!(
        !common::flag(report, "golden.tee_void"),
        "T junction overfilled"
    );
    assert!(
        common::flag(report, "golden.wall_material"),
        "wall junction not filleted"
    );
    assert!(
        !common::flag(report, "golden.wall_void"),
        "wall junction overfilled"
    );
}

/// Every legal height takes the standard divider height above the floor.
#[test]
fn divider_heights_follow_the_standard() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    for (height_u, above_floor, top) in [
        (3, 10.25, 15.0),
        (6, 23.25, 28.0),
        (9, 44.25, 49.0),
        (12, 65.25, 70.0),
    ] {
        let path = format!("heights.{height_u}");
        common::assert_close(
            common::number(report, &format!("{path}.height_above_floor_mm")),
            above_floor,
            &format!("{height_u}U divider height"),
        );
        common::assert_near(
            common::number(report, &format!("{path}.top_mm")),
            top,
            TOP_TOLERANCE,
            &format!("{height_u}U divider top"),
        );
        assert_eq!(
            common::value(report, &format!("{path}.solids")).as_u64(),
            Some(1),
            "{height_u}U body must stay one solid"
        );
        assert!(
            common::flag(report, &format!("{path}.is_closed")),
            "{height_u}U body must stay closed"
        );
    }
}

/// A synthetic tree whose divider face lands on the inner corner arc still
/// builds one closed solid: the 2.0 mm cove at the projected wall face buries
/// the acute notch, leaving no sharp internal corner at the tip.
#[test]
fn a_face_on_the_corner_arc_leaves_no_sharp_corner() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(common::value(report, "corner.solids").as_u64(), Some(1));
    assert!(common::flag(report, "corner.is_valid"));
    assert!(common::flag(report, "corner.is_closed"));

    let tip = [
        common::number(report, "corner.notch_tip_mm.0"),
        common::number(report, "corner.notch_tip_mm.1"),
    ];
    common::assert_close(tip[0], 81.73205080756887, "corner notch tip x");
    common::assert_close(tip[1], 80.25, "corner notch tip y");

    // The tip and the notch beside it are material now; the void beyond the
    // cove surface stays void, so the corner is filleted rather than filled.
    assert!(
        common::flag(report, "corner.notch_tip_material"),
        "the notch tip must be buried in the cove"
    );
    assert!(
        common::flag(report, "corner.notch_material"),
        "the notch beside the tip must be filled"
    );
    assert!(
        common::flag(report, "corner.notch_outside_is_void"),
        "the cove must not swallow the compartment"
    );

    // No section vertex sits at the sharp tip: the cove and the wall arc
    // cross about 0.9 mm away.
    assert!(
        common::number(report, "corner.nearest_vertex_to_tip_mm") > 0.5,
        "a sharp corner vertex remains at the notch tip"
    );

    let circles = common::value(report, "corner.cove_circles")
        .as_array()
        .expect("cove circles");
    assert_eq!(circles.len(), 4, "four wall-end cove columns: {circles:?}");
    assert_cove_circle(circles, [79.75, 76.25], 2.0, "corner cove (+x, near face)");
    assert_cove_circle(circles, [-79.75, 76.25], 2.0, "corner cove (-x, near face)");
    assert_cove_circle(circles, [79.75, 82.25], 2.0, "corner cove (+x, far face)");
    assert_cove_circle(circles, [-79.75, 82.25], 2.0, "corner cove (-x, far face)");

    assert_wire_boxes(
        &[
            [-81.75, -81.75, 81.75, 78.25],
            [-81.374, 80.25, 81.374, 81.75],
        ],
        "corner.section_z10",
        0.01,
        "corner tree compartments at z = 10",
    );
}

/// The same corner handling for a divider whose *cross position* lands on
/// the corner arc (the prototype's `abs(xc) <= inner / 2 - OUTER_RADIUS`
/// case), where a straight wall column would have been skipped.
#[test]
fn a_cross_position_on_the_corner_arc_leaves_no_sharp_corner() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(
        common::value(report, "corner_cross.solids").as_u64(),
        Some(1)
    );
    assert!(common::flag(report, "corner_cross.is_valid"));
    assert!(common::flag(report, "corner_cross.is_closed"));
    assert!(common::flag(report, "corner_cross.notch_tip_material"));
    assert!(common::flag(report, "corner_cross.notch_material"));

    let circles = common::value(report, "corner_cross.cove_circles")
        .as_array()
        .expect("cove circles");
    assert_eq!(circles.len(), 4, "four wall-end cove columns: {circles:?}");
    assert_cove_circle(circles, [76.25, 79.75], 2.0, "cross cove (near face, +y)");
    assert_cove_circle(circles, [82.25, 79.75], 2.0, "cross cove (far face, +y)");

    assert_wire_boxes(
        &[
            [-81.75, -81.75, 78.25, 81.75],
            [80.25, -81.374, 81.75, 81.374],
        ],
        "corner_cross.section_z10",
        0.01,
        "corner-cross tree compartments at z = 10",
    );
}

/// One split of a 4 x 4 body: the divider misses the four centre feet, so
/// this pins the shell's cell webs (without them the body is five solids).
#[test]
fn a_single_split_of_a_four_by_four_body_stays_one_solid() {
    if !common::freecad_required() {
        return;
    }
    let report = report();

    assert_eq!(
        common::value(report, "single_split.solids").as_u64(),
        Some(1)
    );
    assert!(common::flag(report, "single_split.is_valid"));
    assert!(common::flag(report, "single_split.is_closed"));
    assert_wire_boxes(
        &[[-81.75, -81.75, -1.75, 81.75], [0.25, -81.75, 81.75, 81.75]],
        "single_split.section_z20",
        EPS,
        "one-split compartments at z = 20",
    );
}
