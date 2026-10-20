//! Wire DTOs for the local API.
//!
//! The wire schema is deliberately separate from the domain types: JSON is
//! documented and stable here, while [`crate::spec::Spec`] and
//! [`crate::tree::Node`] stay the compiler's model. Every request type is
//! closed (`deny_unknown_fields`), mirroring the YAML spec: the UI can never
//! express wall thickness, radii, or any other fixed value (ADR-0002).
//!
//! # Endpoints
//!
//! | Endpoint | Request | Response |
//! | --- | --- | --- |
//! | `GET /api/health` | — | `{status, system_version}` |
//! | `GET /api/standard` | — | [`StandardResponse`] |
//! | `GET /api/boxes` | — | `{boxes: [BoxSummary]}` |
//! | `POST /api/boxes/{id}/open` | — | `OpenResponse` |
//! | `POST /api/boxes/{id}/save` | `{design: Design}` | `SaveResponse` |
//! | `POST /api/plan` | `{design: Design}` | `PlanResponse` |
//! | `POST /api/fit` | `FitRequest` | `FitResponse` |
//! | `POST /api/build` | `{design: Design}` | 202 [`BuildStatusResponse`] |
//! | `GET /api/build/status` | — | [`BuildStatusResponse`] |
//! | `GET /api/artifacts/{id}` | — | [`ArtifactsResponse`] |
//! | `GET /api/artifacts/{id}/{name}` | — | artifact bytes (the meshes the 3D preview loads) |
//! | `POST /api/export` | `{base: string}` | [`ExportResponse`] |
//! | `POST /api/publish` | `{id: string}` | [`PublishResponse`] |
//! | `POST /api/index` | — | [`IndexResponse`] |
//! | `GET /` and `/assets/{*path}` | — | embedded frontend |
//!
//! Every error response has the shape `{error: string, errors: [string]}`
//! with status 400 (malformed JSON or a usage problem), 404 (unknown box,
//! build, or asset), 409 (the workspace state conflicts: a build is already
//! running, or a build is incomplete for export or publish), or 422 (invalid
//! design or spec, with the schema-path messages).

use serde::{Deserialize, Serialize};

use crate::build::{Artifact, BuildOutcome, BuildReport, BuildStage};
use crate::spec::{self, Contents, Spec};
use crate::tree::{Axis, Node};

use super::Workspace;

/// `{design: Design}` — the shared request body of plan and save.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DesignRequest {
    /// The box design to plan or save.
    pub design: Design,
}

/// The wire representation of one [`crate::spec::Spec`] — the editor payload.
///
/// `design` is the JSON key ticket #16 fixed for this DTO; the domain term is
/// Spec (`GLOSSARY.md`). It carries exactly what a spec can express: the fixed
/// architecture (walls, radii, lip, print settings) has no field here by
/// construction, and `system` is always the compiler's
/// [`spec::SYSTEM_VERSION`] while `coupon` is never expressible — the UI
/// cannot change either (ADR-0002).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Design {
    /// Box ID, also the registry folder name.
    pub id: String,
    /// Footprint `[N, M]` in Gridfinity units.
    pub footprint: [u32; 2],
    /// Height in Gridfinity units.
    pub height_u: u32,
    /// Whether the build includes a lid.
    pub lid: bool,
    /// Optional lid friction ribs; absent means false.
    #[serde(default)]
    pub lid_ribs: bool,
    /// Free text for `notes.md`; absent means none.
    pub notes: Option<String>,
    /// Optional contents record; absent means none.
    pub contents: Option<ContentsDto>,
    /// Root of the division tree.
    pub divisions: Division,
}

/// The optional contents record of a design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContentsDto {
    /// Free-text description.
    pub description: Option<String>,
    /// Soft items and cables use the soft clearances; absent means false.
    #[serde(default)]
    pub soft: bool,
    /// Longest item dimension in millimetres.
    pub max_length_mm: Option<f64>,
    /// Widest item dimension in millimetres.
    pub max_width_mm: Option<f64>,
    /// Tallest item dimension in millimetres.
    pub max_height_mm: Option<f64>,
    /// Contents weight in kilograms.
    pub weight_kg: Option<f64>,
}

/// One division-tree node.
///
/// - leaf: `{"type": "leaf", "compartment": "A"}` (`compartment` may be null)
/// - split: `{"type": "split", "axis": "x", "at_mm": 80, "near": …, "far": …}`
///
/// This mirrors the YAML tree exactly; the schema is closed, so an unknown
/// field is a request error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Division {
    /// A compartment. The name is optional.
    Leaf {
        /// Human-readable compartment name.
        compartment: Option<String>,
    },
    /// A split on one axis.
    Split {
        /// `x` divides the width, `y` divides the depth.
        axis: Axis,
        /// Distance from the parent origin to the near divider face, mm.
        at_mm: u32,
        /// Child on the origin side of the divider.
        near: Box<Division>,
        /// Child on the far side of the divider.
        far: Box<Division>,
    },
}

impl Design {
    /// Builds the domain spec; the system version and the internal coupon
    /// flag are compiler-owned.
    pub(crate) fn to_spec(&self) -> Spec {
        Spec {
            system: spec::SYSTEM_VERSION.to_owned(),
            id: self.id.clone(),
            footprint: self.footprint,
            height_u: self.height_u,
            lid: self.lid,
            lid_ribs: self.lid_ribs,
            notes: self.notes.clone(),
            contents: self.contents.as_ref().map(Contents::from),
            divisions: Node::from(&self.divisions),
            coupon: false,
        }
    }

    /// Projects a parsed spec onto the wire design.
    pub(crate) fn from_spec(spec: &Spec) -> Self {
        Self {
            id: spec.id.clone(),
            footprint: spec.footprint,
            height_u: spec.height_u,
            lid: spec.lid,
            lid_ribs: spec.lid_ribs,
            notes: spec.notes.clone(),
            contents: spec.contents.as_ref().map(ContentsDto::from),
            divisions: Division::from(&spec.divisions),
        }
    }
}

impl From<&ContentsDto> for Contents {
    fn from(contents: &ContentsDto) -> Self {
        Self {
            description: contents.description.clone(),
            soft: contents.soft,
            max_length_mm: contents.max_length_mm,
            max_width_mm: contents.max_width_mm,
            max_height_mm: contents.max_height_mm,
            weight_kg: contents.weight_kg,
        }
    }
}

impl From<&Contents> for ContentsDto {
    fn from(contents: &Contents) -> Self {
        Self {
            description: contents.description.clone(),
            soft: contents.soft,
            max_length_mm: contents.max_length_mm,
            max_width_mm: contents.max_width_mm,
            max_height_mm: contents.max_height_mm,
            weight_kg: contents.weight_kg,
        }
    }
}

impl From<&Division> for Node {
    fn from(division: &Division) -> Self {
        match division {
            Division::Leaf { compartment } => Node::Leaf {
                compartment: compartment.clone(),
            },
            Division::Split {
                axis,
                at_mm,
                near,
                far,
            } => Node::Split {
                axis: *axis,
                at_mm: *at_mm,
                near: Box::new(Node::from(near.as_ref())),
                far: Box::new(Node::from(far.as_ref())),
            },
        }
    }
}

impl From<&Node> for Division {
    fn from(node: &Node) -> Self {
        match node {
            Node::Leaf { compartment } => Self::Leaf {
                compartment: compartment.clone(),
            },
            Node::Split {
                axis,
                at_mm,
                near,
                far,
            } => Self::Split {
                axis: *axis,
                at_mm: *at_mm,
                near: Box::new(Division::from(near.as_ref())),
                far: Box::new(Division::from(far.as_ref())),
            },
        }
    }
}

/// `GET /api/boxes` — the registry list.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct BoxListResponse {
    /// Every box folder that has a `spec.yaml`, sorted by id.
    pub boxes: Vec<BoxSummary>,
}

/// One registry row.
///
/// A spec that does not parse is still listed — the UI must show it — with
/// the folder name as `id`, null fields, and `valid: false`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct BoxSummary {
    /// The spec id when the file parses, else the folder name.
    pub id: String,
    /// The registry folder name.
    pub folder: String,
    /// Footprint when the spec parses.
    pub footprint: Option<[u32; 2]>,
    /// Height when the spec parses.
    pub height_u: Option<u32>,
    /// Lid flag when the spec parses.
    pub lid: Option<bool>,
    /// Friction-ribs flag when the spec parses.
    pub lid_ribs: Option<bool>,
    /// True when the spec parses and validates.
    pub valid: bool,
    /// Rule violations or the parse error; empty when valid.
    pub errors: Vec<String>,
    /// Host path of the spec file, for display.
    pub path: String,
    /// Last modified time, seconds since the Unix epoch (0 when unknown).
    pub mtime: u64,
}

/// `POST /api/boxes/{id}/open` — the spec file plus its parsed design.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct OpenResponse {
    /// Box id: the spec id when it parses, else the registry key.
    pub id: String,
    /// Host path of the spec file, for display.
    pub path: String,
    /// The spec file's exact text.
    pub text: String,
    /// The parsed design, or null when the file does not parse.
    pub design: Option<Design>,
    /// True when the spec parses and validates.
    pub valid: bool,
    /// Rule violations or the parse error; empty when valid.
    pub errors: Vec<String>,
    /// Validation warnings (a valid spec may still warn).
    pub warnings: Vec<String>,
}

/// `POST /api/boxes/{id}/save` — the canonical spec that was written.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct SaveResponse {
    /// The saved box id.
    pub id: String,
    /// Host path of the written spec file, for display.
    pub path: String,
    /// The canonical text written to `spec.yaml`.
    pub text: String,
    /// Validation warnings for the saved spec.
    pub warnings: Vec<String>,
}

/// `POST /api/plan` — the resolved layout for one design.
///
/// The plan is a rendering of [`crate::tree::validate_tree`] output; every
/// rectangle is absolute in the **interior top-view frame**: `x_mm` runs from
/// the -x inner wall face, `y_mm` from the -y inner wall face, z is not part
/// of the rectangles. `body` also reports the outer and total dimensions, so
/// a preview can draw the walls as `(outer - interior) / 2`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct PlanResponse {
    /// True when the design violates no rule.
    pub valid: bool,
    /// Rule violations with schema paths; empty when valid.
    pub errors: Vec<String>,
    /// Advisory findings (a valid design may still warn).
    pub warnings: Vec<String>,
    /// The resolved fixed geometry for this footprint and height.
    pub body: BodyDto,
    /// Compartments in traversal order.
    pub leaves: Vec<LeafRectDto>,
    /// Divider plates in traversal order.
    pub dividers: Vec<DividerRectDto>,
    /// The documented mass and print-time estimate.
    pub estimate: EstimateDto,
}

/// Fixed body geometry (standard §4).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct BodyDto {
    /// Outer width (x).
    pub outer_width_mm: f64,
    /// Outer depth (y).
    pub outer_depth_mm: f64,
    /// Total height including base and lip.
    pub total_height_mm: f64,
    /// Interior width (x) between the walls.
    pub interior_width_mm: f64,
    /// Interior depth (y) between the walls.
    pub interior_depth_mm: f64,
    /// Interior height from the floor to the lip bottom.
    pub interior_height_mm: f64,
    /// Divider height above the floor.
    pub divider_height_mm: f64,
    /// Divider top above the bed (`base zone + divider height`).
    pub divider_top_mm: f64,
}

/// One compartment as an absolute rectangle, with its even-divide options.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct LeafRectDto {
    /// Dotted schema path, for example `divisions.near.far`.
    pub path: String,
    /// Compartment name when present.
    pub compartment: Option<String>,
    /// Min-corner x in the body interior.
    pub x_mm: f64,
    /// Min-corner y in the body interior.
    pub y_mm: f64,
    /// Width (x span).
    pub width_mm: f64,
    /// Depth (y span).
    pub depth_mm: f64,
    /// Even-divide options for the Serve editor, one block per axis.
    pub divide: DivideDto,
}

/// One leaf's even-divide options.
///
/// A block is null on an axis when the leaf has no wall side there, is below
/// the 12 mm minimum in the perpendicular direction, or has no legal count.
/// The planner is [`crate::tree::even_divide`]; this DTO only projects it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct DivideDto {
    /// Options along x, or null.
    pub x: Option<DivideAxisDto>,
    /// Options along y, or null.
    pub y: Option<DivideAxisDto>,
}

/// The even-divide options along one axis.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct DivideAxisDto {
    /// True when the leaf starts on the -x (or -y) inner wall face.
    pub near_wall: bool,
    /// True when the leaf ends on the +x (or +y) inner wall face.
    pub far_wall: bool,
    /// Legal counts in increasing order.
    pub options: Vec<DivideOptionDto>,
}

/// One legal even-divide layout.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct DivideOptionDto {
    /// Number of dividers, so `count + 1` compartments.
    pub count: u32,
    /// Strip widths in origin order, in millimetres.
    pub spans_mm: Vec<f64>,
    /// True when a strip is below the 15 mm warning threshold.
    pub warned: bool,
    /// The exact subtree that replaces the leaf.
    pub division: Division,
    /// Preview rectangles for the dividers, in the interior frame.
    pub dividers: Vec<RectDto>,
}

/// One divider plate as an absolute rectangle.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct DividerRectDto {
    /// Split axis: `x` plates are `thickness` wide, `y` plates deep.
    pub axis: Axis,
    /// Dotted schema path of the split, for example `divisions.near`.
    pub path: String,
    /// Near-face x (the plate's min corner).
    pub x_mm: f64,
    /// Plate min-corner y.
    pub y_mm: f64,
    /// Width (x span).
    pub width_mm: f64,
    /// Depth (y span).
    pub depth_mm: f64,
    /// The parent region this split divides, for drag bounds in the editor.
    pub parent: RectDto,
}

/// One axis-aligned rectangle in the body interior frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub(crate) struct RectDto {
    /// Min-corner x in the body interior.
    pub x_mm: f64,
    /// Min-corner y in the body interior.
    pub y_mm: f64,
    /// Width (x span).
    pub width_mm: f64,
    /// Depth (y span).
    pub depth_mm: f64,
}

/// The documented print estimate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct EstimateDto {
    /// Estimated solid volume.
    pub volume_mm3: f64,
    /// Estimated mass.
    pub mass_g: f64,
    /// Estimated print time.
    pub print_hours: f64,
}

impl BodyDto {
    /// Resolves the body block from fixed geometry.
    pub(crate) fn from_geometry(geometry: &crate::tree::BodyGeometry) -> Self {
        // Keep the wire values at the standard's precision; subtracting a
        // divider gap from 36.45 must not surface as 26.450000000000003.
        let round6 = |value: f64| (value * 1_000_000.0).round() / 1_000_000.0;
        let divider_height_mm = round6(geometry.divider_height_mm);
        Self {
            outer_width_mm: geometry.outer_width_mm,
            outer_depth_mm: geometry.outer_depth_mm,
            total_height_mm: round6(geometry.total_height_mm),
            interior_width_mm: geometry.interior_width_mm,
            interior_depth_mm: geometry.interior_depth_mm,
            interior_height_mm: round6(geometry.interior_height_mm),
            divider_height_mm,
            divider_top_mm: round6(crate::tree::FLOOR_TOP_MM + divider_height_mm),
        }
    }
}

impl LeafRectDto {
    /// Projects one validated leaf, resolving its even-divide options against
    /// the body interior.
    pub(crate) fn from_leaf(leaf: &crate::tree::Leaf, body: &crate::tree::BodyGeometry) -> Self {
        Self {
            path: leaf.path.clone(),
            compartment: leaf.compartment.clone(),
            x_mm: leaf.x_mm,
            y_mm: leaf.y_mm,
            width_mm: leaf.width_mm,
            depth_mm: leaf.depth_mm,
            divide: DivideDto::from(&crate::tree::even_divide(body, leaf)),
        }
    }
}

impl From<&crate::tree::EvenDivide> for DivideDto {
    fn from(divide: &crate::tree::EvenDivide) -> Self {
        Self {
            x: divide.x.as_ref().map(DivideAxisDto::from),
            y: divide.y.as_ref().map(DivideAxisDto::from),
        }
    }
}

impl From<&crate::tree::AxisDivide> for DivideAxisDto {
    fn from(axis: &crate::tree::AxisDivide) -> Self {
        Self {
            near_wall: axis.near_wall,
            far_wall: axis.far_wall,
            options: axis.options.iter().map(DivideOptionDto::from).collect(),
        }
    }
}

impl From<&crate::tree::DivideOption> for DivideOptionDto {
    fn from(option: &crate::tree::DivideOption) -> Self {
        Self {
            count: option.count,
            spans_mm: option.spans_mm.clone(),
            warned: option.warned,
            division: Division::from(&option.division),
            dividers: option.dividers.iter().map(RectDto::from).collect(),
        }
    }
}

impl From<&crate::tree::Rect> for RectDto {
    fn from(rect: &crate::tree::Rect) -> Self {
        Self {
            x_mm: rect.x_mm,
            y_mm: rect.y_mm,
            width_mm: rect.width_mm,
            depth_mm: rect.depth_mm,
        }
    }
}

impl From<&crate::tree::Divider> for DividerRectDto {
    fn from(divider: &crate::tree::Divider) -> Self {
        Self {
            axis: divider.axis,
            path: divider.path.clone(),
            x_mm: divider.x_mm,
            y_mm: divider.y_mm,
            width_mm: divider.width_mm,
            depth_mm: divider.depth_mm,
            parent: RectDto::from(&divider.parent),
        }
    }
}

impl From<crate::tree::Estimate> for EstimateDto {
    fn from(estimate: crate::tree::Estimate) -> Self {
        // The estimate is a planning figure, not a measured artifact; keep
        // the wire values at six decimals so f64 accumulation noise does not
        // leak into the API/golden JSON.
        let round6 = |value: f64| (value * 1_000_000.0).round() / 1_000_000.0;
        Self {
            volume_mm3: round6(estimate.volume_mm3),
            mass_g: round6(estimate.mass_g),
            print_hours: round6(estimate.print_hours),
        }
    }
}

/// `POST /api/fit` — one item's dimensions and kind.
///
/// The item is checked with the standard §8 contents rules exactly as a
/// spec's `contents` record is: `width_mm` against the interior width,
/// `length_mm` against the usable diagonal, `height_mm` against the interior
/// height, and `weight_kg` against the per-body limit. The endpoint does not
/// try rotated orientations; it answers with the spec rules as written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FitRequest {
    /// Longest item dimension in millimetres; may lie diagonally.
    pub length_mm: f64,
    /// Widest item dimension in millimetres.
    pub width_mm: f64,
    /// Tallest item dimension in millimetres.
    pub height_mm: f64,
    /// Item weight in kilograms; absent means unknown.
    pub weight_kg: Option<f64>,
    /// Soft items and cables; absent means rigid.
    #[serde(default)]
    pub soft: bool,
}

/// `POST /api/fit` — the viable footprint and height combinations.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct FitResponse {
    /// The item is soft (cables, textiles).
    pub soft: bool,
    /// False when the weight alone exceeds the 10 kg per-body limit.
    pub weight_ok: bool,
    /// The standard §8 clearances applied to this item.
    pub clearances: FitClearancesDto,
    /// Viable combinations, sorted by footprint area, then height.
    pub viable: Vec<FitCandidateDto>,
}

/// The standard §8 clearances applied to one item.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct FitClearancesDto {
    /// Added on each side.
    pub per_side_mm: f64,
    /// Added above the item.
    pub height_clearance_mm: f64,
    /// Headroom kept for diagonal placement.
    pub diagonal_margin_mm: f64,
}

/// One viable footprint and height combination.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct FitCandidateDto {
    /// Footprint `[N, M]` in Gridfinity units.
    pub footprint: [u32; 2],
    /// Height in Gridfinity units.
    pub height_u: u32,
}

/// `GET /api/standard` — every fixed value the system panel shows read-only.
///
/// All numbers are sourced from the Rust constants the validator and the
/// estimates use; those constants mirror the Python master (`cad/*.py`), so
/// the panel shows the generator's fixed architecture rather than a second
/// source of values.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct StandardResponse {
    /// The one system version this compiler accepts.
    pub system_version: &'static str,
    /// Shell and body architecture (standard §4).
    pub walls: WallsDto,
    /// Divider architecture (standard §4.1/§4.3).
    pub dividers: DividersDto,
    /// Inset friction lid (standard §4.4).
    pub lid: LidDto,
    /// The pinned Gridfinity interface (standard §3).
    pub gridfinity: GridfinityDto,
    /// Variable-surface limits and estimate thresholds.
    pub limits: LimitsDto,
    /// Contents clearances (standard §8).
    pub contents: ContentsClearancesDto,
    /// The one material the standard is calibrated for (standard §5).
    pub material: MaterialDto,
    /// The coupon calibration this toolchain is anchored on.
    pub calibration: CalibrationDto,
}

/// Shell and body architecture values (standard §4).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct WallsDto {
    /// Wall thickness.
    pub wall_mm: f64,
    /// Pinned base profile height below the floor slab (standard §3).
    pub base_zone_mm: f64,
    /// Floor slab above the base profile (standard §4, ADR-0014).
    pub floor_mm: f64,
    /// Outer vertical corner radius.
    pub outer_radius_mm: f64,
    /// Inner vertical corner radius.
    pub inner_radius_mm: f64,
    /// Floor-to-wall fillet radius.
    pub floor_fillet_mm: f64,
}

/// Divider architecture values (standard §4.1/§4.3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct DividersDto {
    /// Every divider is this thick.
    pub thickness_mm: f64,
    /// Top-edge chamfer.
    pub top_chamfer_mm: f64,
    /// Divider-to-floor and divider-to-wall cove radius.
    pub cove_mm: f64,
    /// Smallest legal leaf dimension.
    pub min_leaf_mm: u32,
    /// Leaves below this size warn.
    pub warn_leaf_mm: u32,
    /// Maximum splits on one root-to-leaf path.
    pub max_depth: u32,
    /// Per-height divider top, above the bed (3U/6U/9U/12U: 19/32/53/74).
    pub tops_mm: Vec<DividerTopDto>,
}

/// One height's divider geometry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct DividerTopDto {
    /// Body height in Gridfinity units.
    pub height_u: u32,
    /// Divider height above the floor.
    pub divider_height_mm: f64,
    /// Divider top above the bed.
    pub top_mm: f64,
}

/// Inset friction lid values (standard §4.4).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct LidDto {
    /// Plate thickness.
    pub plate_mm: f64,
    /// Skirt thickness and depth.
    pub skirt_mm: f64,
    /// Skirt clearance to the lip, per side.
    pub clearance_per_side_mm: f64,
    /// Plate outer vertical corner radius.
    pub corner_radius_mm: f64,
    /// Plate top-edge roundover.
    pub top_roundover_mm: f64,
    /// Skirt bottom chamfer.
    pub bottom_chamfer_mm: f64,
    /// Finger-hole diameter.
    pub finger_hole_diameter_mm: f64,
    /// Finger-hole centre distance from the +y edge.
    pub finger_hole_edge_distance_mm: f64,
    /// Optional friction-rib interference.
    pub rib_interference_mm: f64,
}

/// Pinned Gridfinity interface numbers (standard §3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct GridfinityDto {
    /// Grid pitch.
    pub grid_mm: f64,
    /// Height unit.
    pub height_unit_mm: f64,
    /// Clearance between parts, per side.
    pub clearance_per_side_mm: f64,
    /// Nominal outer width of a 1-unit body.
    pub nominal_width_1u_mm: f64,
    /// Base profile height.
    pub base_height_mm: f64,
    /// Stacking lip height.
    pub lip_height_mm: f64,
}

/// Variable-surface limits and estimate thresholds.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct LimitsDto {
    /// Smallest footprint side, in Gridfinity units.
    pub min_footprint_units: u32,
    /// Largest footprint side: the 4 × 4 module.
    pub max_footprint_units: u32,
    /// Largest outer dimension the printer guard accepts.
    pub max_outer_mm: f64,
    /// Heights the schema accepts.
    pub heights_u: &'static [u32],
    /// Default height for a new box.
    pub default_height_u: u32,
    /// Contents weight limit per body.
    pub max_contents_kg: f64,
    /// Estimated mass above this value warns.
    pub max_mass_warning_g: f64,
    /// Estimated print time above this value warns.
    pub max_print_hours: f64,
}

/// Standard §8 contents clearances.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ContentsClearancesDto {
    /// Rigid items: 2 mm per side, 5 mm above.
    pub rigid: ClearanceDto,
    /// Soft items and cables: 5 mm per side, 10 mm above.
    pub soft: ClearanceDto,
    /// Headroom when an item lies diagonally.
    pub diagonal_margin_mm: f64,
}

/// One clearance pair.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ClearanceDto {
    /// Added on each side.
    pub per_side_mm: f64,
    /// Added above the item.
    pub above_mm: f64,
}

/// The material the standard is calibrated for (standard §5).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct MaterialDto {
    /// Filament name.
    pub name: &'static str,
    /// Density used by the mass estimate.
    pub density_g_per_cm3: f64,
    /// Effective flow used by the print-time estimate.
    pub effective_flow_mm3_per_s: f64,
}

/// The coupon calibration this toolchain is anchored on.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct CalibrationDto {
    /// The internal coupon id.
    pub coupon_id: &'static str,
    /// The SYS-2.0 calibration approval date.
    pub date: &'static str,
    /// Filament used.
    pub material: &'static str,
    /// Nozzle used.
    pub nozzle: &'static str,
    /// Where the procedure and record live.
    pub result: &'static str,
}

/// `POST /api/build` and `GET /api/build/status` — the one build job.
///
/// The job is one at a time; a second `POST /api/build` while a build runs is
/// rejected with 409. There is no queue and no cancel: the state machine is
/// exactly these four states.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum BuildStatusResponse {
    /// No build has run in this server process.
    Idle,
    /// One build is running.
    Running {
        /// Box id being built.
        id: String,
        /// Current phase: `preparing`, `building`, or `verifying`.
        stage: BuildStage,
        /// Milliseconds since the build started.
        elapsed_ms: u64,
    },
    /// The last build finished and every invariant held.
    Done {
        /// The structured outcome.
        outcome: Box<BuildOutcomeDto>,
    },
    /// The last build failed.
    Failed {
        /// The exit-contract message (for example `build failed for …`).
        error: String,
        /// The raw `freecadcmd` output tail; empty when the process produced
        /// none (the message still names the failing stage).
        diagnostic: String,
    },
}

/// The structured outcome of one successful build.
///
/// This is [`crate::build::BuildOutcome`] with every path mapped to the host
/// path the user sees; the nested `report` is the typed projection of
/// `report.json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct BuildOutcomeDto {
    /// Box id.
    pub id: String,
    /// Build directory as the host sees it.
    pub out_dir: String,
    /// The verified measurement report.
    pub report: BuildReport,
    /// Files in the build directory, in the canonical order.
    pub artifacts: Vec<ArtifactDto>,
    /// Registry folder a publish copied into, when there was one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_dir: Option<String>,
}

/// One file with its size and the path the host sees.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ArtifactDto {
    /// File name.
    pub name: String,
    /// Host path, for display and for opening in the slicer.
    pub path: String,
    /// Size in bytes.
    pub bytes: u64,
}

/// `GET /api/artifacts/{id}` — the files in one build directory.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ArtifactsResponse {
    /// Box id.
    pub id: String,
    /// Build directory as the host sees it.
    pub out_dir: String,
    /// Regular files directly in the build directory, sorted by name.
    pub artifacts: Vec<ArtifactDto>,
}

/// `POST /api/export` — which build to export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExportRequest {
    /// Box id: the build at `build/<base>` is copied to `exports/<base>`.
    pub base: String,
}

/// `POST /api/export` — the named copies that were written.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ExportResponse {
    /// Export base name.
    pub base: String,
    /// Export directory as the host sees it.
    pub out_dir: String,
    /// Files written, in the canonical order.
    pub artifacts: Vec<ArtifactDto>,
}

/// `POST /api/publish` — which box to publish.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublishRequest {
    /// Box id in the registry.
    pub id: String,
}

/// `POST /api/publish` — where the artifacts were copied.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct PublishResponse {
    /// Box id.
    pub id: String,
    /// Registry folder as the host sees it.
    pub published_dir: String,
}

/// `POST /api/index` — the regenerated registry index.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct IndexResponse {
    /// Host path of the written `INDEX.md`.
    pub path: String,
    /// Number of boxes recorded.
    pub boxes: usize,
}

impl ArtifactDto {
    /// Projects one build or export artifact, mapping the path to the host.
    pub(crate) fn from_artifact(workspace: &Workspace, artifact: &Artifact) -> Self {
        Self {
            name: artifact.name.clone(),
            path: workspace.display(&artifact.path),
            bytes: artifact.bytes,
        }
    }
}

impl BuildOutcomeDto {
    /// Projects a finished build for the wire.
    pub(crate) fn from_outcome(workspace: &Workspace, outcome: &BuildOutcome) -> Self {
        Self {
            id: outcome.id.clone(),
            out_dir: workspace.display(&outcome.out_dir),
            report: outcome.report.clone(),
            artifacts: outcome
                .artifacts
                .iter()
                .map(|artifact| ArtifactDto::from_artifact(workspace, artifact))
                .collect(),
            published_dir: outcome
                .published_dir
                .as_deref()
                .map(|path| workspace.display(path)),
        }
    }
}
