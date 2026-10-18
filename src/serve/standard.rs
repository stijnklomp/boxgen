//! `GET /api/standard`: the read-only fixed values.
//!
//! Every number is sourced from the Rust constants in [`crate::tree`] and
//! [`crate::spec`] that the validator and the estimates use. Each of those
//! constants mirrors its Python counterpart in `cad/*.py` — named in the
//! constant's own comment — and the two sides move together, so the panel
//! shows the generator's fixed architecture rather than a second source of
//! values (ADR-0002: the UI owns no geometry and no rules). The calibration
//! block is the coupon record the current SYS-1.0 toolchain is accepted
//! against (`docs/spec/docs/calibration.md`; the coupon passed on 2026-10-18
//! per ticket #16).

use axum::Json;

use crate::build::COUPON_ID;
use crate::spec;
use crate::tree;

use super::dto::{
    CalibrationDto, ClearanceDto, ContentsClearancesDto, DividerTopDto, DividersDto, GridfinityDto,
    LidDto, LimitsDto, MaterialDto, StandardResponse, WallsDto,
};

/// The calibration record: the day the coupon passed all nine tests.
const CALIBRATION_DATE: &str = "2026-10-18";

/// The filament the architecture and the estimates are calibrated for.
const MATERIAL_NAME: &str = "Bambu Lab PETG Basic";

/// The nozzle the calibration and the print recipe assume.
const NOZZLE: &str = "0.4 mm hardened steel";

/// Where the nine-test procedure and the human record live.
const CALIBRATION_RESULT: &str = "passed: all nine tests in docs/spec/docs/calibration.md";

/// `GET /api/standard`.
pub(crate) async fn get() -> Json<StandardResponse> {
    Json(standard())
}

/// Builds the fixed-value panel from the Rust constants.
pub(crate) fn standard() -> StandardResponse {
    StandardResponse {
        system_version: spec::SYSTEM_VERSION,
        walls: WallsDto {
            wall_mm: tree::WALL_MM,
            base_zone_mm: tree::BASE_HEIGHT_MM,
            outer_radius_mm: tree::OUTER_RADIUS_MM,
            inner_radius_mm: tree::INNER_RADIUS_MM,
            floor_fillet_mm: tree::FLOOR_FILLET_MM,
        },
        dividers: DividersDto {
            thickness_mm: tree::DIVIDER_MM,
            top_chamfer_mm: tree::DIVIDER_TOP_CHAMFER_MM,
            cove_mm: tree::DIVIDER_COVE_MM,
            min_leaf_mm: tree::MIN_LEAF_MM,
            warn_leaf_mm: tree::WARN_LEAF_MM,
            max_depth: tree::MAX_TREE_DEPTH,
            tops_mm: divider_tops(),
        },
        lid: LidDto {
            plate_mm: tree::LID_PLATE_MM,
            skirt_mm: tree::LID_SKIRT_MM,
            clearance_per_side_mm: tree::LID_CLEARANCE_MM,
            corner_radius_mm: tree::LID_CORNER_RADIUS_MM,
            top_roundover_mm: tree::LID_TOP_ROUNDOVER_MM,
            bottom_chamfer_mm: tree::LID_SKIRT_CHAMFER_MM,
            finger_hole_diameter_mm: tree::LID_FINGER_HOLE_DIAMETER_MM,
            finger_hole_edge_distance_mm: tree::LID_FINGER_HOLE_EDGE_DISTANCE_MM,
            rib_interference_mm: tree::LID_RIB_MM,
        },
        gridfinity: GridfinityDto {
            grid_mm: tree::GRID_MM,
            height_unit_mm: tree::HEIGHT_UNIT_MM,
            clearance_per_side_mm: tree::OUTER_CLEARANCE_MM / 2.0,
            nominal_width_1u_mm: tree::GRID_MM - tree::OUTER_CLEARANCE_MM,
            base_height_mm: tree::BASE_HEIGHT_MM,
            lip_height_mm: tree::LIP_HEIGHT_MM,
        },
        limits: LimitsDto {
            min_footprint_units: tree::MIN_FOOTPRINT_UNITS,
            max_footprint_units: tree::MAX_FOOTPRINT_UNITS,
            max_outer_mm: tree::MAX_OUTER_MM,
            heights_u: &tree::HEIGHTS_U,
            default_height_u: tree::DEFAULT_HEIGHT_U,
            max_contents_kg: spec::MAX_CONTENTS_KG,
            max_mass_warning_g: tree::MAX_MASS_G,
            max_print_hours: tree::MAX_PRINT_HOURS,
        },
        contents: ContentsClearancesDto {
            rigid: ClearanceDto {
                per_side_mm: spec::CONTENTS_CLEARANCE_PER_SIDE_MM,
                above_mm: spec::CONTENTS_HEIGHT_CLEARANCE_MM,
            },
            soft: ClearanceDto {
                per_side_mm: spec::CONTENTS_SOFT_CLEARANCE_PER_SIDE_MM,
                above_mm: spec::CONTENTS_SOFT_HEIGHT_CLEARANCE_MM,
            },
            diagonal_margin_mm: spec::CONTENTS_DIAGONAL_MARGIN_MM,
        },
        material: MaterialDto {
            name: MATERIAL_NAME,
            density_g_per_cm3: tree::PETG_DENSITY_G_PER_CM3,
            effective_flow_mm3_per_s: tree::EFFECTIVE_FLOW_MM3_PER_S,
        },
        calibration: CalibrationDto {
            coupon_id: COUPON_ID,
            date: CALIBRATION_DATE,
            material: MATERIAL_NAME,
            nozzle: NOZZLE,
            result: CALIBRATION_RESULT,
        },
    }
}

/// The divider top per accepted height, above the bed.
///
/// The divider top is the solid base zone plus the divider height above the
/// floor, which is why the panel can derive it from the same constants the
/// geometry uses instead of listing four literals.
fn divider_tops() -> Vec<DividerTopDto> {
    tree::HEIGHTS_U
        .iter()
        .map(|&height_u| {
            let divider_height_mm = tree::BodyGeometry::new([1, 1], height_u).divider_height_mm;
            DividerTopDto {
                height_u,
                divider_height_mm,
                top_mm: tree::BASE_HEIGHT_MM + divider_height_mm,
            }
        })
        .collect()
}
