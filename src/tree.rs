//! Division tree, leaf sizes, and validation.
//!
//! Contract: `docs/spec/docs/generator.md` §4. A node is either a leaf (an optional
//! compartment name) or a split on one axis into a near and a far child. The
//! tree is the only stored form; grid views are renderings.
//!
//! Tree types and the validation rules land with ticket #4, dividers with
//! ticket #8, and the in-place `divide` helper with ticket #12. This module
//! owns all of them so the schema and geometry code share one tree model. The
//! `divide` helper edits the YAML losslessly: only the target node's text is
//! rewritten, so surrounding comments and formatting survive.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use yaml_edit::{Document, Mapping, YamlBuilder, YamlFile, YamlNode};

use crate::error::BoxgenError;
use crate::spec;

/// Gridfinity grid pitch in millimetres; mirrors `cad/gridfinity.py::GRID`.
pub const GRID_MM: f64 = 42.0;

/// Clearance subtracted from the nominal footprint (0.25 mm per side);
/// `2 × cad/gridfinity.py::CLEARANCE`.
pub const OUTER_CLEARANCE_MM: f64 = 0.5;

/// Body, lip, floor and divider wall thickness (standard §4); mirrors
/// `cad/shell.py::WALL`.
pub const WALL_MM: f64 = 2.0;

/// Gridfinity height unit in millimetres; mirrors `cad/shell.py::HEIGHT_UNIT`.
pub const HEIGHT_UNIT_MM: f64 = 7.0;

/// Base zone height, standard §3: 0.8 bottom chamfer + 1.8 vertical + 2.15
/// top chamfer; mirrors `cad/gridfinity.py::BASE_HEIGHT`.
pub const BASE_HEIGHT_MM: f64 = 4.75;

/// Official Gridfinity stacking lip height, standard §3: 0.7 mm bottom
/// chamfer, 1.8 mm vertical, 1.9 mm top chamfer; mirrors
/// `cad/gridfinity.py::LIP_HEIGHT`. The lip sits above the nominal body
/// height; it is not subtracted from the interior.
pub const LIP_HEIGHT_MM: f64 = 4.4;

/// Floor slab above the pinned base profile, standard §4; mirrors
/// `cad/shell.py::FLOOR`. The profile's top face cannot be continuous across
/// the per-cell feet (the 0.5 mm grooves must stay open to the baseplate's
/// mating plane), so the interior floor is this slab on top of it (ADR-0014).
pub const FLOOR_MM: f64 = 0.8;

/// The interior floor surface above the bed: the base profile plus the floor
/// slab; mirrors `cad/shell.py::FLOOR_TOP`.
pub const FLOOR_TOP_MM: f64 = BASE_HEIGHT_MM + FLOOR_MM;

/// Nominal stack pitch for one body on another: the body height (U × 7)
/// minus the measured 0.35 mm base seat. Mirrors the measured
/// `cad/tests/lid.py::_measure_stack` value; lids are top-only.
pub const STACK_SEAT_MM: f64 = 0.35;

/// Divider-to-lip gap on 3U bodies, standard §4.3; mirrors
/// `cad/dividers.py::DIVIDER_GAP_3U`.
pub const DIVIDER_GAP_3U_MM: f64 = 2.0;

/// Divider-to-lip gap on 6U and taller bodies, standard §4.3; mirrors
/// `cad/dividers.py::DIVIDER_GAP`.
pub const DIVIDER_GAP_MM: f64 = 10.0;

/// Lid plate thickness, standard §4.4; mirrors `cad/lid.py::LID_PLATE`.
pub const LID_PLATE_MM: f64 = 2.0;

/// Lid skirt thickness and depth, standard §4.4; mirrors
/// `cad/lid.py::LID_SKIRT`.
pub const LID_SKIRT_MM: f64 = 2.0;

/// Largest outer dimension the X2D guard accepts, contract §7.
pub const MAX_OUTER_MM: f64 = 200.0;

/// Every divider is 2.0 mm thick (contract §4); mirrors
/// `cad/dividers.py::DIVIDER_THICKNESS`.
pub const DIVIDER_MM: f64 = 2.0;

/// Divider top-edge chamfer (standard §4.1); mirrors
/// `cad/dividers.py::DIVIDER_TOP_CHAMFER`.
pub const DIVIDER_TOP_CHAMFER_MM: f64 = 0.4;

/// Smallest legal leaf dimension in millimetres (contract §4).
pub const MIN_LEAF_MM: u32 = 12;

/// Leaves below this size are valid but warned about (contract §4).
pub const WARN_LEAF_MM: u32 = 15;

/// Maximum number of splits on one root-to-leaf path (contract §4).
pub const MAX_TREE_DEPTH: u32 = 32;

/// PETG Basic density used by the estimate, standard §5; mirrored by
/// `cad/build.py::PETG_DENSITY_G_PER_CM3`.
pub const PETG_DENSITY_G_PER_CM3: f64 = 1.27;

/// Conservative effective extrusion throughput used by the estimate.
///
/// PETG on a 0.4 mm nozzle at 0.20 mm layers does not sustain the slicer's
/// peak volumetric flow over a whole print. This figure averages walls,
/// shells, infill and travel; it is an estimate input, not a slicer setting.
/// Mirrored by `cad/build.py::EFFECTIVE_FLOW_MM3_PER_S`.
pub const EFFECTIVE_FLOW_MM3_PER_S: f64 = 5.5;

/// Estimated mass above this value is a warning (contract §7).
pub const MAX_MASS_G: f64 = 550.0;

/// Estimated print time above this value is a warning (contract §7).
pub const MAX_PRINT_HOURS: f64 = 22.0;

/// Smallest footprint side the schema accepts, in Gridfinity units (contract §3).
pub const MIN_FOOTPRINT_UNITS: u32 = 1;

/// Largest footprint side the schema accepts: the 4 × 4 module (contract §3,
/// standard §9/§12).
pub const MAX_FOOTPRINT_UNITS: u32 = 4;

/// Heights the schema accepts (contract §3).
pub const HEIGHTS_U: [u32; 4] = [3, 6, 9, 12];

/// Default height for a new box (standard §9: 6U is the universal default).
pub const DEFAULT_HEIGHT_U: u32 = 6;

/// Outer vertical corner radius (standard §4.1): the pinned Gridfinity outer
/// radius; mirrors `cad/gridfinity.py::OUTER_RADIUS`.
pub const OUTER_RADIUS_MM: f64 = 3.75;

/// Inner vertical corner radius (standard §4.1): `OUTER_RADIUS_MM - WALL_MM`,
/// which keeps the 2.0 mm wall uniform around corners; mirrors
/// `cad/shell.py::INNER_RADIUS`.
pub const INNER_RADIUS_MM: f64 = OUTER_RADIUS_MM - WALL_MM;

/// Floor-to-wall fillet radius (standard §4.1); mirrors
/// `cad/shell.py::FLOOR_FILLET`.
pub const FLOOR_FILLET_MM: f64 = 2.0;

/// Divider-to-floor and divider-to-wall cove radius (standard §4.1): the same
/// 2.0 mm radius as the floor fillet, built as box-minus-quarter-cylinder
/// coves in `cad/dividers.py`; mirrors `cad/dividers.py::JUNCTION_FILLET`
/// (= `cad/shell.py::FLOOR_FILLET`).
pub const DIVIDER_COVE_MM: f64 = FLOOR_FILLET_MM;

/// Lid skirt clearance per side to the lip (standard §4.4/§6); mirrors
/// `cad/lid.py::LID_CLEARANCE`.
pub const LID_CLEARANCE_MM: f64 = 0.30;

/// Lid outer vertical corner radius (standard §4.1): the lip's 1.75 mm inner
/// corner minus clearance, rounded down to 1.40 mm for extra corner
/// clearance; mirrors `cad/lid.py::LID_CORNER_RADIUS`.
pub const LID_CORNER_RADIUS_MM: f64 = 1.40;

/// Lid plate top-edge roundover (standard §4.1/§4.4); mirrors
/// `cad/lid.py::LID_TOP_ROUNDOVER`.
pub const LID_TOP_ROUNDOVER_MM: f64 = 1.0;

/// Lid skirt bottom chamfer for self-alignment (standard §4.1/§4.4); mirrors
/// `cad/lid.py::LID_SKIRT_CHAMFER`.
pub const LID_SKIRT_CHAMFER_MM: f64 = 0.6;

/// Lid finger hole diameter (standard §4.4); mirrors
/// `cad/lid.py::FINGER_HOLE_DIAMETER`.
pub const LID_FINGER_HOLE_DIAMETER_MM: f64 = 20.0;

/// Finger-hole centre distance from the +y outer edge (standard §4.4);
/// mirrors `cad/lid.py::FINGER_HOLE_EDGE_DISTANCE`.
pub const LID_FINGER_HOLE_EDGE_DISTANCE_MM: f64 = 14.0;

/// Optional friction-rib interference (standard §4.4/§6); mirrors
/// `cad/lid.py::LID_RIB_PROUD`.
pub const LID_RIB_MM: f64 = 0.15;

/// Split axis. `x` splits the width, `y` splits the depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    /// Vertical divider; splits the parent's width.
    X,
    /// Horizontal divider; splits the parent's depth.
    Y,
}

/// Formats a millimetre dimension without trailing zeros.
///
/// Shared by the validate report (`src/spec.rs`) and the generated notes
/// (`src/build.rs`) so the two renderings cannot drift; keep the trimming
/// rule in this one place.
pub(crate) fn fmt_mm(value: f64) -> String {
    let formatted = format!("{value:.2}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

impl Axis {
    /// The axis spelling used by the spec schema.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
        }
    }
}

/// One node of the division tree: a leaf or a split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// A compartment. The name is optional but must be unique when present.
    Leaf {
        /// Human-readable compartment name.
        compartment: Option<String>,
    },
    /// A split into a near and a far child.
    Split {
        /// Axis whose parent dimension is split.
        axis: Axis,
        /// Distance from the parent origin to the divider's near face.
        at_mm: u32,
        /// Child on the origin side of the divider.
        near: Box<Node>,
        /// Child on the far side of the divider.
        far: Box<Node>,
    },
}

impl Node {
    /// Builds a leaf node.
    pub fn leaf(compartment: Option<&str>) -> Self {
        Self::Leaf {
            compartment: compartment.map(str::to_owned),
        }
    }

    /// Builds a split node.
    pub fn split(axis: Axis, at_mm: u32, near: Node, far: Node) -> Self {
        Self::Split {
            axis,
            at_mm,
            near: Box::new(near),
            far: Box::new(far),
        }
    }
}

/// Fixed body geometry for one footprint and height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyGeometry {
    /// Outer width (x) in millimetres.
    pub outer_width_mm: f64,
    /// Outer depth (y) in millimetres.
    pub outer_depth_mm: f64,
    /// Total height (z) including base and lip.
    pub total_height_mm: f64,
    /// Usable interior width (x) between the walls.
    pub interior_width_mm: f64,
    /// Usable interior depth (y) between the walls.
    pub interior_depth_mm: f64,
    /// Usable interior height (z) from the floor to the lip bottom.
    pub interior_height_mm: f64,
    /// Divider height above the floor.
    pub divider_height_mm: f64,
}

impl BodyGeometry {
    /// Resolves the fixed dimensions from the two variable spec fields.
    pub fn new(footprint: [u32; 2], height_u: u32) -> Self {
        let outer_width_mm = f64::from(footprint[0]) * GRID_MM - OUTER_CLEARANCE_MM;
        let outer_depth_mm = f64::from(footprint[1]) * GRID_MM - OUTER_CLEARANCE_MM;
        let body_height_mm = f64::from(height_u) * HEIGHT_UNIT_MM;
        let total_height_mm = body_height_mm + LIP_HEIGHT_MM;
        let interior_height_mm = body_height_mm - FLOOR_TOP_MM;
        let divider_gap_mm = if height_u == 3 {
            DIVIDER_GAP_3U_MM
        } else {
            DIVIDER_GAP_MM
        };

        Self {
            outer_width_mm,
            outer_depth_mm,
            total_height_mm,
            interior_width_mm: outer_width_mm - 2.0 * WALL_MM,
            interior_depth_mm: outer_depth_mm - 2.0 * WALL_MM,
            interior_height_mm,
            // Grouped as one subtraction so the f64 result is the clean
            // decimal for every accepted height (26.45, 13.45, ...).
            divider_height_mm: interior_height_mm - divider_gap_mm,
        }
    }

    /// Reports any outer dimension above the printer guard limit.
    pub fn printer_limit_violations(&self) -> Vec<String> {
        let mut violations = Vec::new();

        for (axis, value) in [
            ("width", self.outer_width_mm),
            ("depth", self.outer_depth_mm),
            ("height", self.total_height_mm),
        ] {
            if value > MAX_OUTER_MM {
                let limit = MAX_OUTER_MM;
                violations.push(format!(
                    "body: outer {axis} {value:.1} mm exceeds the {limit:.0} mm printer limit"
                ));
            }
        }

        violations
    }

    /// Simple, deliberately conservative print estimate for one body and lid.
    ///
    /// Assumptions:
    /// - The body is a closed prism: outer box volume minus the interior
    ///   cavity. The base zone below the floor is solid (ADR-0013) and the
    ///   0.8 mm floor slab sits on it (ADR-0014), so the
    ///   estimate treats the interior as a single void; the rounded corners,
    ///   the base grooves, and the lip still make it an overestimate.
    /// - Every divider is 2.0 mm thick, spans its parent rectangle, and is
    ///   [`BodyGeometry::divider_height_mm`] tall.
    /// - The lid adds a 2.0 mm plate plus a 2.0 mm x 2.0 mm skirt; the 20 mm
    ///   finger hole is ignored.
    /// - PETG Basic density is 1.27 g/cm3.
    /// - Effective extrusion throughput is 5.5 mm3/s. Travel, acceleration,
    ///   cooling and slicer settings are not modelled.
    pub fn estimate(&self, divider_volume_mm3: f64, lid: bool) -> Estimate {
        let outer_volume_mm3 = self.outer_width_mm * self.outer_depth_mm * self.total_height_mm;
        let interior_volume_mm3 = self.interior_width_mm.max(0.0)
            * self.interior_depth_mm.max(0.0)
            * self.interior_height_mm.max(0.0);
        let shell_volume_mm3 = (outer_volume_mm3 - interior_volume_mm3).max(0.0);
        let lid_volume_mm3 = if lid {
            self.outer_width_mm * self.outer_depth_mm * LID_PLATE_MM
                + 2.0 * (self.outer_width_mm + self.outer_depth_mm) * LID_SKIRT_MM * LID_SKIRT_MM
        } else {
            0.0
        };
        let volume_mm3 = shell_volume_mm3 + divider_volume_mm3.max(0.0) + lid_volume_mm3;

        Estimate {
            volume_mm3,
            mass_g: volume_mm3 / 1000.0 * PETG_DENSITY_G_PER_CM3,
            print_hours: volume_mm3 / EFFECTIVE_FLOW_MM3_PER_S / 3600.0,
        }
    }
}

/// Estimated printed mass and time for one body and lid.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Estimate {
    /// Estimated solid volume in cubic millimetres.
    pub volume_mm3: f64,
    /// Estimated mass in grams.
    pub mass_g: f64,
    /// Estimated print time in hours.
    pub print_hours: f64,
}

/// One validated leaf, with the path it occupied in the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Leaf {
    /// Dotted schema path, for example `divisions.near.far`.
    pub path: String,
    /// Optional compartment name.
    pub compartment: Option<String>,
    /// Leaf origin x (the min corner) in the body interior, millimetres from
    /// the -x inner wall face.
    pub x_mm: f64,
    /// Leaf origin y (the min corner) in the body interior, millimetres from
    /// the -y inner wall face.
    pub y_mm: f64,
    /// Leaf width (x) in millimetres.
    pub width_mm: f64,
    /// Leaf depth (y) in millimetres.
    pub depth_mm: f64,
}

/// One divider plate, positioned absolutely in the body interior.
///
/// The rectangle is the plate's top-view footprint in the same interior frame
/// as [`Leaf`]: `x_mm`/`y_mm` are the min corner, and the plate spans the
/// whole parent region across its axis. `parent` is that region, so a consumer
/// (the local UI) can clamp a drag without re-walking the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Divider {
    /// Dotted schema path of the split, for example `divisions.near`.
    pub path: String,
    /// Axis the divider splits: `x` is `DIVIDER_MM` wide, `y` is
    /// `DIVIDER_MM` deep.
    pub axis: Axis,
    /// Near face x (the plate's min corner) in the body interior, in
    /// millimetres.
    pub x_mm: f64,
    /// Plate min corner y in the body interior, in millimetres.
    pub y_mm: f64,
    /// Plate width (x span) in millimetres.
    pub width_mm: f64,
    /// Plate depth (y span) in millimetres.
    pub depth_mm: f64,
    /// The parent region this split divides.
    pub parent: Rect,
}

/// An axis-aligned rectangle in the body interior frame.
///
/// `x_mm`/`y_mm` are the min corner, measured from the -x/-y inner wall
/// faces.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// Min corner x in the body interior.
    pub x_mm: f64,
    /// Min corner y in the body interior.
    pub y_mm: f64,
    /// Width (x span).
    pub width_mm: f64,
    /// Depth (y span).
    pub depth_mm: f64,
}

/// Result of validating one division tree.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TreeReport {
    /// Leaves in traversal order; near before far.
    pub leaves: Vec<Leaf>,
    /// Divider plates in traversal order; near before far.
    pub dividers: Vec<Divider>,
    /// Rule violations, with schema paths.
    pub errors: Vec<String>,
    /// Advisory findings that do not fail the run.
    pub warnings: Vec<String>,
    /// Sum of all divider volumes in cubic millimetres.
    pub divider_volume_mm3: f64,
}

impl TreeReport {
    /// True when no rule violation was found.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Validates a division tree against the body interior.
///
/// `body.interior_width_mm` is the root width (x) and
/// `body.interior_depth_mm` the root depth (y). Absolute divider positions are
/// tracked through both children so a divider cannot repeat an ancestor's
/// position on the same axis.
pub fn validate_tree(body: &BodyGeometry, divisions: &Node) -> TreeReport {
    let mut report = TreeReport::default();
    let mut walker = Walker {
        report: &mut report,
        divider_height_mm: body.divider_height_mm,
        ancestors: Vec::new(),
        seen: BTreeMap::new(),
    };

    walker.visit(
        divisions,
        "divisions",
        Region {
            width: body.interior_width_mm,
            depth: body.interior_depth_mm,
            x: 0.0,
            y: 0.0,
        },
        0,
    );

    report
}

/// A rectangle inside the body interior.
#[derive(Debug, Clone, Copy)]
struct Region {
    width: f64,
    depth: f64,
    x: f64,
    y: f64,
}

struct Walker<'a> {
    report: &'a mut TreeReport,
    divider_height_mm: f64,
    ancestors: Vec<(Axis, f64)>,
    seen: BTreeMap<String, String>,
}

impl Walker<'_> {
    fn visit(&mut self, node: &Node, path: &str, region: Region, level: u32) {
        if level > MAX_TREE_DEPTH {
            let limit = MAX_TREE_DEPTH;
            self.report.errors.push(format!(
                "{path}: tree depth {level} exceeds the {limit} limit"
            ));
            return;
        }

        match node {
            Node::Leaf { compartment } => {
                self.check_size(path, region, "leaf");
                self.check_leaf_warning(path, region);

                if let Some(name) = compartment {
                    let first = self.seen.get(name).cloned();
                    if let Some(first) = first {
                        self.report.errors.push(format!(
                            "{path}: duplicate compartment {name:?} (first at {first})"
                        ));
                    } else {
                        self.seen.insert(name.clone(), path.to_owned());
                    }
                }

                self.report.leaves.push(Leaf {
                    path: path.to_owned(),
                    compartment: compartment.clone(),
                    x_mm: region.x,
                    y_mm: region.y,
                    width_mm: region.width,
                    depth_mm: region.depth,
                });
            }
            Node::Split {
                axis,
                at_mm,
                near,
                far,
            } => {
                self.check_size(path, region, "split region");

                let minimum = MIN_LEAF_MM;
                if *at_mm < minimum {
                    self.report.errors.push(format!(
                        "{path}: at_mm {at_mm} is below the {minimum} mm minimum"
                    ));
                }

                let at_mm = f64::from(*at_mm);
                let position = match axis {
                    Axis::X => region.x + at_mm,
                    Axis::Y => region.y + at_mm,
                };
                if self.ancestors.iter().any(|(ancestor_axis, ancestor)| {
                    ancestor_axis == axis && (ancestor - position).abs() < 1e-9
                }) {
                    let axis_name = axis.as_str();
                    self.report.errors.push(format!(
                        "{path}: divider on {axis_name} at {position:.1} mm coincides with an ancestor divider"
                    ));
                }

                let (thickness, span) = match axis {
                    Axis::X => (DIVIDER_MM, region.depth),
                    Axis::Y => (region.width, DIVIDER_MM),
                };
                self.report.divider_volume_mm3 += thickness * span * self.divider_height_mm;
                let parent = Rect {
                    x_mm: region.x,
                    y_mm: region.y,
                    width_mm: region.width,
                    depth_mm: region.depth,
                };
                let divider = match axis {
                    Axis::X => Divider {
                        path: path.to_owned(),
                        axis: *axis,
                        x_mm: position,
                        y_mm: region.y,
                        width_mm: DIVIDER_MM,
                        depth_mm: region.depth,
                        parent,
                    },
                    Axis::Y => Divider {
                        path: path.to_owned(),
                        axis: *axis,
                        x_mm: region.x,
                        y_mm: position,
                        width_mm: region.width,
                        depth_mm: DIVIDER_MM,
                        parent,
                    },
                };
                self.report.dividers.push(divider);

                self.ancestors.push((*axis, position));
                match axis {
                    Axis::X => {
                        let near_width = at_mm;
                        let far_width = region.width - at_mm - DIVIDER_MM;
                        self.visit(
                            near,
                            &format!("{path}.near"),
                            Region {
                                width: near_width,
                                ..region
                            },
                            level + 1,
                        );
                        self.visit(
                            far,
                            &format!("{path}.far"),
                            Region {
                                width: far_width,
                                x: region.x + at_mm + DIVIDER_MM,
                                ..region
                            },
                            level + 1,
                        );
                    }
                    Axis::Y => {
                        let near_depth = at_mm;
                        let far_depth = region.depth - at_mm - DIVIDER_MM;
                        self.visit(
                            near,
                            &format!("{path}.near"),
                            Region {
                                depth: near_depth,
                                ..region
                            },
                            level + 1,
                        );
                        self.visit(
                            far,
                            &format!("{path}.far"),
                            Region {
                                depth: far_depth,
                                y: region.y + at_mm + DIVIDER_MM,
                                ..region
                            },
                            level + 1,
                        );
                    }
                }
                self.ancestors.pop();
            }
        }
    }

    fn check_size(&mut self, path: &str, region: Region, noun: &str) {
        let minimum = MIN_LEAF_MM;
        if region.width < f64::from(minimum) {
            self.report.errors.push(format!(
                "{path}: {noun} {width:.1} mm wide is below the {minimum} mm minimum",
                width = region.width,
            ));
        }
        if region.depth < f64::from(minimum) {
            self.report.errors.push(format!(
                "{path}: {noun} {depth:.1} mm deep is below the {minimum} mm minimum",
                depth = region.depth,
            ));
        }
    }

    fn check_leaf_warning(&mut self, path: &str, region: Region) {
        let warning = f64::from(WARN_LEAF_MM);
        let minimum = f64::from(MIN_LEAF_MM);
        let large_enough = region.width >= minimum && region.depth >= minimum;
        if large_enough && (region.width < warning || region.depth < warning) {
            let threshold = WARN_LEAF_MM;
            self.report.warnings.push(format!(
                "{path}: leaf {width:.1} x {depth:.1} mm is below the {threshold} mm warning threshold",
                width = region.width,
                depth = region.depth,
            ));
        }
    }
}

/// Edits one split in a spec's division tree in place.
///
/// `path` addresses a node as `root` or with `.near`/`.far` segments, for
/// example `root.near` or `root.far.near`; omitting it targets the root.
///
/// - A split target accepts `--axis` and/or `--at`; the omitted field keeps
///   its current value. Supplying neither is a usage error.
/// - A leaf target becomes a split and needs both `--axis` and `--at`. A named
///   leaf keeps its compartment on the near child; the far child is unnamed.
/// - An unresolved path is an invalid-spec error (exit 2).
///
/// The edited text is re-parsed and re-validated before it is written. Invalid
/// results and every lookup failure leave the file byte-for-byte unchanged.
/// Only the target node is rewritten: the lossless YAML editor preserves the
/// rest of the file's comments and formatting.
pub fn divide(
    spec_path: &Path,
    path: Option<&str>,
    axis: Option<&str>,
    at_mm: Option<u32>,
) -> Result<(), BoxgenError> {
    let text = fs::read_to_string(spec_path).map_err(|error| BoxgenError::InvalidSpec {
        path: spec_path.to_path_buf(),
        message: format!("cannot read spec: {error}"),
    })?;

    let edited = divide_text(&text, path, axis, at_mm).map_err(|error| {
        let message = error.message().to_owned();
        match error {
            DivideError::Usage(_) => BoxgenError::Usage { message },
            DivideError::Invalid(_) => BoxgenError::InvalidSpec {
                path: spec_path.to_path_buf(),
                message,
            },
        }
    })?;

    fs::write(spec_path, edited).map_err(|error| BoxgenError::Io {
        message: format!("cannot write spec `{}`: {error}", spec_path.display()),
    })
}

/// A failed divide edit, before the command maps it to the process contract.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DivideError {
    /// The flags cannot produce a meaningful edit; exit code 1.
    Usage(String),
    /// The path or edited spec is invalid; exit code 2.
    Invalid(String),
}

impl DivideError {
    fn message(&self) -> &str {
        match self {
            Self::Usage(message) | Self::Invalid(message) => message,
        }
    }
}

/// One `.near` or `.far` step in a divide path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathStep {
    Near,
    Far,
}

impl PathStep {
    const fn key(self) -> &'static str {
        match self {
            Self::Near => "near",
            Self::Far => "far",
        }
    }
}

/// Parses `root`, `root.near`, `root.far.near`, ... into steps.
fn parse_target_path(path: Option<&str>) -> Result<Vec<PathStep>, String> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    if path == "root" {
        return Ok(Vec::new());
    }

    let mut segments = path.split('.');
    if segments.next() != Some("root") {
        return Err(format!("`{path}` must be `root` or start with `root.`"));
    }

    let mut steps = Vec::new();
    for segment in segments {
        match segment {
            "near" => steps.push(PathStep::Near),
            "far" => steps.push(PathStep::Far),
            "" => return Err(format!("`{path}` has an empty path segment")),
            other => {
                return Err(format!(
                    "`{path}` contains unknown segment `{other}` (expected `near` or `far`)"
                ));
            }
        }
    }
    Ok(steps)
}

fn display_path(steps: &[PathStep]) -> String {
    let mut path = String::from("root");
    for step in steps {
        path.push('.');
        path.push_str(step.key());
    }
    path
}

/// Resolves a path against the parsed tree, reporting where it stops.
fn resolve_target<'a>(root: &'a Node, steps: &[PathStep]) -> Result<&'a Node, String> {
    let mut node = root;
    let mut prefix = String::from("root");

    for step in steps {
        let Node::Split { near, far, .. } = node else {
            let name = match node {
                Node::Leaf {
                    compartment: Some(name),
                } => format!(" (compartment {name:?})"),
                Node::Leaf { compartment: None } => String::new(),
                Node::Split { .. } => String::new(),
            };
            return Err(format!(
                "`{prefix}` is a leaf{name}, so `{prefix}.{}` does not exist",
                step.key()
            ));
        };
        node = match step {
            PathStep::Near => near.as_ref(),
            PathStep::Far => far.as_ref(),
        };
        prefix.push('.');
        prefix.push_str(step.key());
    }

    Ok(node)
}

fn resolve_target_mut<'a>(
    mut node: &'a mut Node,
    steps: &[PathStep],
) -> Result<&'a mut Node, String> {
    for step in steps {
        node = match node {
            Node::Split { near, far, .. } => match step {
                PathStep::Near => near.as_mut(),
                PathStep::Far => far.as_mut(),
            },
            Node::Leaf { .. } => {
                return Err("the path runs through a leaf".to_owned());
            }
        };
    }
    Ok(node)
}

fn parse_target_axis(value: &str) -> Result<Axis, DivideError> {
    match value {
        "x" => Ok(Axis::X),
        "y" => Ok(Axis::Y),
        other => Err(DivideError::Usage(format!(
            "unknown axis `{other}` (expected `x` or `y`)"
        ))),
    }
}

/// Builds an empty or named leaf as a flow mapping.
fn flow_leaf(compartment: Option<&str>) -> YamlNode {
    let document = Document::from_str("{}\n").expect("an empty flow mapping is valid YAML");
    let mapping = document
        .as_mapping()
        .expect("an empty flow mapping is a mapping");

    if let Some(compartment) = compartment {
        let scalar = YamlBuilder::scalar(compartment)
            .build()
            .document()
            .expect("a scalar builder produces a document")
            .as_scalar()
            .expect("a scalar builder produces a scalar");
        mapping.set("compartment", YamlNode::Scalar(scalar));
    }

    YamlNode::Mapping(mapping)
}

/// Builds a split node in the style of its parent mapping.
fn split_node(axis: Axis, at_mm: u32, compartment: Option<&str>, flow: bool) -> YamlNode {
    if flow {
        let mapping = Mapping::new_flow();
        mapping.set("axis", axis.as_str());
        mapping.set("at_mm", i64::from(at_mm));
        mapping.set("near", flow_leaf(compartment));
        mapping.set("far", flow_leaf(None));
        return YamlNode::Mapping(mapping);
    }

    let template = format!("axis: {}\nat_mm: {at_mm}\n", axis.as_str());
    let document =
        Document::from_str(&template).expect("the generated split template is valid YAML");
    let mapping = document
        .as_mapping()
        .expect("the generated split template is a mapping");
    mapping.set("near", flow_leaf(compartment));
    mapping.set("far", flow_leaf(None));
    YamlNode::Mapping(mapping)
}

/// Finds the parent mapping, child key, and value node for a tree path.
fn locate_yaml_target(
    file: &YamlFile,
    steps: &[PathStep],
) -> Result<(Mapping, String, YamlNode), String> {
    let document = file
        .document()
        .ok_or_else(|| "the YAML file has no document".to_owned())?;
    let mut parent = document
        .as_mapping()
        .ok_or_else(|| "the YAML document root is not a mapping".to_owned())?;
    let mut key = String::from("divisions");
    let mut value = parent
        .get(key.as_str())
        .ok_or_else(|| "the spec has no `divisions` mapping".to_owned())?;

    for step in steps {
        let mapping = value
            .as_mapping()
            .ok_or_else(|| format!("`{key}` is not a mapping"))?
            .clone();
        parent = mapping;
        key = step.key().to_owned();
        value = parent
            .get(key.as_str())
            .ok_or_else(|| format!("the spec has no `{key}` key"))?;
    }

    Ok((parent, key, value))
}

/// Applies one divide edit to spec YAML and returns the edited text.
///
/// Kept separate from [`divide`] so the lossless edit and its validation can
/// be tested without touching the filesystem.
fn divide_text(
    text: &str,
    path: Option<&str>,
    axis: Option<&str>,
    at_mm: Option<u32>,
) -> Result<String, DivideError> {
    let original = spec::parse(text)
        .map_err(|error| DivideError::Invalid(format!("YAML parse error: {error}")))?;

    let steps = parse_target_path(path).map_err(|reason| {
        DivideError::Invalid(format!("path `{}`: {reason}", path.unwrap_or("root")))
    })?;
    let target_path = display_path(&steps);
    let target = resolve_target(&original.divisions, &steps).map_err(|reason| {
        DivideError::Invalid(format!("path `{target_path}` does not resolve: {reason}"))
    })?;

    let target_is_split = matches!(target, Node::Split { .. });
    let compartment = match target {
        Node::Leaf { compartment } => compartment.clone(),
        Node::Split { .. } => None,
    };

    if target_is_split && axis.is_none() && at_mm.is_none() {
        return Err(DivideError::Usage(format!(
            "path `{target_path}` is a split: provide --axis and/or --at (neither was given)"
        )));
    }
    if !target_is_split && (axis.is_none() || at_mm.is_none()) {
        return Err(DivideError::Usage(format!(
            "path `{target_path}` is a leaf: provide both --axis and --at to replace it with a split"
        )));
    }

    let new_axis = axis.map(parse_target_axis).transpose()?;
    let new_at = at_mm;

    let mut expected = original.clone();
    let target_mut = resolve_target_mut(&mut expected.divisions, &steps).map_err(|reason| {
        DivideError::Invalid(format!("path `{target_path}` does not resolve: {reason}"))
    })?;

    if target_is_split {
        let Node::Split {
            axis: current_axis,
            at_mm: current_at,
            ..
        } = target_mut
        else {
            return Err(DivideError::Invalid(format!(
                "path `{target_path}` changed between resolution and edit"
            )));
        };
        if let Some(axis) = new_axis {
            *current_axis = axis;
        }
        if let Some(at_mm) = new_at {
            *current_at = at_mm;
        }
    } else {
        let axis = new_axis.expect("a leaf target requires --axis");
        let at_mm = new_at.expect("a leaf target requires --at");
        *target_mut = Node::split(
            axis,
            at_mm,
            Node::leaf(compartment.as_deref()),
            Node::leaf(None),
        );
    }

    let file = YamlFile::from_str(text)
        .map_err(|error| DivideError::Invalid(format!("cannot parse YAML for editing: {error}")))?;
    let (parent, key, value) = locate_yaml_target(&file, &steps).map_err(|reason| {
        DivideError::Invalid(format!(
            "cannot locate `{target_path}` in the YAML: {reason}"
        ))
    })?;

    if target_is_split {
        let mapping = value.as_mapping().ok_or_else(|| {
            DivideError::Invalid(format!("`{target_path}` is not a YAML mapping"))
        })?;
        if let Some(axis) = new_axis {
            mapping.set("axis", axis.as_str());
        }
        if let Some(at_mm) = new_at {
            mapping.set("at_mm", i64::from(at_mm));
        }
    } else {
        let split = split_node(
            new_axis.expect("a leaf target requires --axis"),
            new_at.expect("a leaf target requires --at"),
            compartment.as_deref(),
            parent.is_flow_style(),
        );
        parent.set(key, split);
    }

    let edited_text = file.to_string();
    let edited = spec::parse(&edited_text).map_err(|error| {
        DivideError::Invalid(format!("the edited spec does not parse: {error}"))
    })?;

    if edited != expected {
        return Err(DivideError::Invalid(format!(
            "the YAML edit for `{target_path}` did not produce the requested division tree"
        )));
    }

    let validation = edited.validate();
    if !validation.is_valid() {
        let count = validation.errors.len();
        let noun = if count == 1 { "error" } else { "errors" };
        return Err(DivideError::Invalid(format!(
            "path `{target_path}` produced {count} validation {noun}:\n  - {}",
            validation.errors.join("\n  - ")
        )));
    }

    Ok(edited_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> BodyGeometry {
        BodyGeometry::new([4, 4], 6)
    }

    fn small_body() -> BodyGeometry {
        BodyGeometry::new([1, 1], 3)
    }

    fn leaf(name: &str) -> Node {
        Node::leaf(Some(name))
    }

    fn unnamed() -> Node {
        Node::leaf(None)
    }

    fn split(axis: Axis, at_mm: u32, near: Node, far: Node) -> Node {
        Node::split(axis, at_mm, near, far)
    }

    fn report(root: &Node) -> TreeReport {
        validate_tree(&body(), root)
    }

    fn errors(root: &Node) -> Vec<String> {
        report(root).errors
    }

    fn has_error(root: &Node, needle: &str) -> bool {
        errors(root).iter().any(|error| error.contains(needle))
    }

    fn assert_mm(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected} mm, got {actual} mm"
        );
    }

    #[test]
    fn body_dimensions_match_the_catalog() {
        let body = body();
        assert_mm(body.outer_width_mm, 167.5);
        assert_mm(body.outer_depth_mm, 167.5);
        assert_mm(body.total_height_mm, 46.4);
        assert_mm(body.interior_width_mm, 163.5);
        assert_mm(body.interior_depth_mm, 163.5);
        assert_mm(body.interior_height_mm, 36.45);
        assert_mm(body.divider_height_mm, 26.45);

        let small = small_body();
        assert_mm(small.outer_width_mm, 41.5);
        assert_mm(small.total_height_mm, 25.4);
        assert_mm(small.interior_width_mm, 37.5);
        assert_mm(small.interior_depth_mm, 37.5);
        assert_mm(small.interior_height_mm, 15.45);
        assert_mm(small.divider_height_mm, 13.45);
    }

    /// The fixed-architecture constants mirror the frozen SYS-2.0 standard
    /// (`docs/spec/docs/standard.md`). They are pinned against the document,
    /// not against each other, so a silent re-typing fails here instead of
    /// becoming a second source of truth.
    #[test]
    fn fixed_architecture_constants_match_the_standard() {
        assert_eq!(GRID_MM, 42.0);
        assert_eq!(OUTER_CLEARANCE_MM, 0.5);
        assert_eq!(HEIGHT_UNIT_MM, 7.0);
        assert_eq!(BASE_HEIGHT_MM, 4.75);
        assert_eq!(FLOOR_MM, 0.8);
        assert_eq!(FLOOR_TOP_MM, 5.55);
        assert_eq!(LIP_HEIGHT_MM, 4.4);
        assert_eq!(STACK_SEAT_MM, 0.35);
        assert_eq!(WALL_MM, 2.0);
        assert_eq!(OUTER_RADIUS_MM, 3.75);
        assert_eq!(INNER_RADIUS_MM, 1.75);
        assert_eq!(FLOOR_FILLET_MM, 2.0);
        assert_eq!(DIVIDER_MM, 2.0);
        assert_eq!(DIVIDER_TOP_CHAMFER_MM, 0.4);
        assert_eq!(DIVIDER_COVE_MM, 2.0);
        assert_eq!(LID_PLATE_MM, 2.0);
        assert_eq!(LID_SKIRT_MM, 2.0);
        assert_eq!(LID_CLEARANCE_MM, 0.30);
        assert_eq!(LID_CORNER_RADIUS_MM, 1.40);
        assert_eq!(LID_TOP_ROUNDOVER_MM, 1.0);
        assert_eq!(LID_SKIRT_CHAMFER_MM, 0.6);
        assert_eq!(LID_FINGER_HOLE_DIAMETER_MM, 20.0);
        assert_eq!(LID_FINGER_HOLE_EDGE_DISTANCE_MM, 14.0);
        assert_eq!(LID_RIB_MM, 0.15);
        assert_eq!((MIN_FOOTPRINT_UNITS, MAX_FOOTPRINT_UNITS), (1, 4));
        assert_eq!(HEIGHTS_U, [3, 6, 9, 12]);
        assert_eq!(DEFAULT_HEIGHT_U, 6);
        assert_eq!(MIN_LEAF_MM, 12);
        assert_eq!(WARN_LEAF_MM, 15);
        assert_eq!(MAX_TREE_DEPTH, 32);
        assert_eq!(MAX_OUTER_MM, 200.0);
    }

    #[test]
    fn valid_tree_reports_leaves_and_no_issues() {
        let root = split(
            Axis::X,
            80,
            split(Axis::Y, 60, leaf("USB-C short"), leaf("USB-C long")),
            leaf("Power"),
        );

        let report = report(&root);

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.leaves.len(), 3);
        assert_eq!(report.leaves[0].path, "divisions.near.near");
        assert_eq!(report.leaves[0].compartment.as_deref(), Some("USB-C short"));
    }

    #[test]
    fn at_mm_below_the_minimum_is_an_error() {
        let root = split(Axis::X, 8, leaf("A"), leaf("B"));
        let errors = errors(&root);

        assert!(
            errors.iter().any(|error| error.contains("at_mm 8")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("leaf 8.0 mm wide")),
            "the near child must also be rejected: {errors:?}"
        );
    }

    #[test]
    fn far_child_below_the_minimum_is_an_error() {
        let root = split(Axis::X, 160, leaf("A"), leaf("B"));

        assert!(has_error(&root, "leaf 1.5 mm wide"), "{:?}", errors(&root));
    }

    #[test]
    fn leaf_below_the_minimum_in_width_is_an_error() {
        let report = validate_tree(
            &small_body(),
            &split(
                Axis::X,
                12,
                leaf("A"),
                split(Axis::X, 12, leaf("B"), leaf("C")),
            ),
        );

        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("leaf 9.5 mm wide")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn leaf_below_the_minimum_in_depth_is_an_error() {
        let report = validate_tree(
            &small_body(),
            &split(
                Axis::Y,
                12,
                leaf("A"),
                split(Axis::Y, 12, leaf("B"), leaf("C")),
            ),
        );

        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("leaf 9.5 mm deep")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn leaf_below_fifteen_warns_without_failing() {
        let root = split(Axis::X, 14, leaf("Thin"), leaf("Wide"));
        let report = report(&root);

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.warnings.len(), 1);
        assert!(
            report.warnings[0].contains("15 mm"),
            "{:?}",
            report.warnings
        );
        assert!(report.warnings[0].contains("divisions.near"));
    }

    #[test]
    fn a_fifteen_millimetre_leaf_does_not_warn() {
        let report = report(&split(Axis::X, 15, leaf("Edge"), leaf("Wide")));

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    #[test]
    fn duplicate_compartment_names_are_an_error() {
        let root = split(Axis::X, 20, leaf("A"), leaf("A"));
        let errors = errors(&root);

        assert!(
            errors
                .iter()
                .any(|error| error.contains("duplicate compartment \"A\"")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_tree_deeper_than_thirty_two_levels_is_an_error() {
        let mut root = unnamed();
        for _ in 0..40 {
            root = split(Axis::X, 12, unnamed(), root);
        }

        assert!(has_error(&root, "tree depth"), "{:?}", errors(&root));
        assert!(has_error(&root, "32"), "{:?}", errors(&root));
    }

    #[test]
    fn a_tree_thirty_two_levels_deep_is_allowed() {
        let mut root = unnamed();
        for _ in 0..32 {
            root = split(Axis::X, 12, unnamed(), root);
        }

        // A synthetic wide body keeps every split legal, so only the depth
        // guard can fail here.
        let wide = BodyGeometry {
            interior_width_mm: 1000.0,
            interior_depth_mm: 1000.0,
            ..body()
        };
        let report = validate_tree(&wide, &root);

        assert!(
            !report
                .errors
                .iter()
                .any(|error| error.contains("tree depth")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn a_divider_coincident_with_an_ancestor_is_an_error() {
        let root = split(
            Axis::X,
            18,
            split(Axis::X, 18, leaf("A"), leaf("B")),
            leaf("C"),
        );

        assert!(
            has_error(&root, "coincides with an ancestor divider"),
            "{:?}",
            errors(&root)
        );
    }

    #[test]
    fn the_same_position_on_the_other_axis_is_allowed() {
        let root = split(
            Axis::X,
            18,
            split(Axis::Y, 18, leaf("A"), leaf("B")),
            leaf("C"),
        );

        assert!(
            !has_error(&root, "coincides"),
            "x and y positions are independent: {:?}",
            errors(&root)
        );
    }

    #[test]
    fn absolute_offsets_are_tracked_through_both_children() {
        // The near child keeps the parent origin, so a split at 20 is
        // coincident with the root's divider at 20.
        let root = split(
            Axis::X,
            20,
            split(
                Axis::Y,
                15,
                split(Axis::X, 20, leaf("A"), leaf("B")),
                leaf("C"),
            ),
            leaf("D"),
        );

        assert!(
            has_error(&root, "coincides with an ancestor divider"),
            "{:?}",
            errors(&root)
        );
    }

    #[test]
    fn divider_volume_is_summed_over_the_tree() {
        let report = report(&split(
            Axis::X,
            80,
            split(Axis::Y, 60, leaf("A"), leaf("B")),
            leaf("C"),
        ));

        assert_mm(
            report.divider_volume_mm3,
            (7341.15 + 3592.0) * 26.45 / 22.45,
        );
    }

    #[test]
    fn estimate_is_labelled_and_plausible() {
        let report = report(&split(Axis::X, 80, leaf("A"), leaf("B")));
        let with_lid = body().estimate(report.divider_volume_mm3, true);
        let without_lid = body().estimate(report.divider_volume_mm3, false);

        assert!(
            (300.0..550.0).contains(&with_lid.mass_g),
            "mass {} g",
            with_lid.mass_g
        );
        assert!(
            (10.0..22.0).contains(&with_lid.print_hours),
            "print time {} h",
            with_lid.print_hours
        );
        assert!(without_lid.mass_g < with_lid.mass_g);
        assert!(without_lid.print_hours < with_lid.print_hours);
    }

    #[test]
    fn the_printer_limit_guard_flags_outsized_bodies() {
        assert!(body().printer_limit_violations().is_empty());

        let oversized = BodyGeometry {
            outer_width_mm: 201.0,
            ..body()
        };
        let violations = oversized.printer_limit_violations();

        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("200"), "{violations:?}");
    }

    #[test]
    fn a_root_region_below_the_minimum_is_an_error() {
        let tiny = BodyGeometry {
            interior_width_mm: 10.0,
            interior_depth_mm: 10.0,
            ..body()
        };
        let report = validate_tree(&tiny, &unnamed());

        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("leaf 10.0 mm wide")),
            "{:?}",
            report.errors
        );
    }

    const DIVIDE_SPEC: &str = include_str!("../tests/fixtures/4x4-6u.yaml");

    const DIVIDE_SIMPLE_SPEC: &str = "\
system: SYS-2.0
id: 4x4-6U-divide-simple-v1
footprint: [4, 4]
height_u: 6
lid: false
divisions:
  axis: x
  at_mm: 80
  near: { compartment: \"A\" }
  far: { compartment: \"B\" }
";

    const DIVIDE_LEAF_SPEC: &str = "\
system: SYS-2.0
id: 1x1-3u-divide-v1
footprint: [1, 1]
height_u: 3
lid: false
divisions: { compartment: \"Bits\" }
";

    const DIVIDE_UNNAMED_LEAF_SPEC: &str = "\
system: SYS-2.0
id: 1x1-3u-divide-v1
footprint: [1, 1]
height_u: 3
lid: false
divisions: {}
";

    fn divide_ok(text: &str, path: Option<&str>, axis: Option<&str>, at_mm: Option<u32>) -> String {
        divide_text(text, path, axis, at_mm)
            .unwrap_or_else(|error| panic!("expected a successful divide, got {error:?}"))
    }

    fn divide_error(
        text: &str,
        path: Option<&str>,
        axis: Option<&str>,
        at_mm: Option<u32>,
    ) -> DivideError {
        match divide_text(text, path, axis, at_mm) {
            Ok(edited) => panic!("expected a divide error, got:\n{edited}"),
            Err(error) => error,
        }
    }

    #[test]
    fn divide_edits_a_root_split_axis_and_position() {
        let edited = divide_ok(DIVIDE_SIMPLE_SPEC, None, Some("y"), Some(40));
        let expected = DIVIDE_SIMPLE_SPEC
            .replacen("  axis: x\n", "  axis: y\n", 1)
            .replacen("  at_mm: 80\n", "  at_mm: 40\n", 1);

        assert_eq!(edited, expected);
        assert_eq!(
            divide_ok(DIVIDE_SIMPLE_SPEC, Some("root"), Some("y"), Some(40)),
            expected
        );
    }

    #[test]
    fn divide_handles_a_flow_style_parent() {
        let text = "\
system: SYS-2.0
id: 4x4-6U-divide-flow-v1
footprint: [4, 4]
height_u: 6
lid: false
divisions: { axis: x, at_mm: 80, near: { compartment: \"A\" }, far: { compartment: \"B\" } }
";
        let edited = divide_ok(text, Some("root.near"), Some("y"), Some(40));
        let spec = crate::spec::parse(&edited).expect("edited spec must parse");

        let Node::Split { near, .. } = &spec.divisions else {
            panic!("root must stay a split");
        };
        let Node::Split {
            axis,
            at_mm,
            near: leaf_near,
            far: leaf_far,
        } = near.as_ref()
        else {
            panic!("root.near must become a split");
        };

        assert_eq!(*axis, Axis::Y);
        assert_eq!(*at_mm, 40);
        assert_eq!(**leaf_near, Node::leaf(Some("A")));
        assert_eq!(**leaf_far, Node::leaf(None));
        assert!(
            edited.contains("divisions: { axis: x"),
            "the flow parent must stay flow-style:\n{edited}"
        );
    }

    #[test]
    fn divide_edits_only_the_requested_split_field() {
        let axis_only = divide_ok(DIVIDE_SIMPLE_SPEC, None, Some("y"), None);
        let expected_axis = DIVIDE_SIMPLE_SPEC.replacen("  axis: x\n", "  axis: y\n", 1);
        assert_eq!(axis_only, expected_axis);

        let position_only = divide_ok(DIVIDE_SIMPLE_SPEC, None, None, Some(40));
        let expected_position = DIVIDE_SIMPLE_SPEC.replacen("  at_mm: 80\n", "  at_mm: 40\n", 1);
        assert_eq!(position_only, expected_position);
    }

    #[test]
    fn divide_resolves_nested_split_paths() {
        let edited = divide_ok(DIVIDE_SPEC, Some("root.near"), Some("x"), Some(40));
        let expected = DIVIDE_SPEC.replacen(
            "    axis: y\n    at_mm: 60\n",
            "    axis: x\n    at_mm: 40\n",
            1,
        );

        assert_eq!(edited, expected);
    }

    #[test]
    fn divide_turns_a_named_root_leaf_into_a_split() {
        let edited = divide_ok(DIVIDE_LEAF_SPEC, None, Some("x"), Some(18));
        let spec = crate::spec::parse(&edited).expect("edited spec must parse");

        match spec.divisions {
            Node::Split {
                axis,
                at_mm,
                near,
                far,
            } => {
                assert_eq!(axis, Axis::X);
                assert_eq!(at_mm, 18);
                assert_eq!(*near, Node::leaf(Some("Bits")));
                assert_eq!(*far, Node::leaf(None));
            }
            other => panic!("expected a split, got {other:?}"),
        }

        assert!(edited.contains("near: {compartment: Bits}"), "{edited}");
        assert!(edited.contains("far: {}"), "{edited}");
    }

    #[test]
    fn divide_turns_an_unnamed_leaf_into_an_unnamed_split() {
        let edited = divide_ok(DIVIDE_UNNAMED_LEAF_SPEC, None, Some("y"), Some(18));
        let spec = crate::spec::parse(&edited).expect("edited spec must parse");

        match spec.divisions {
            Node::Split { near, far, .. } => {
                assert_eq!(*near, Node::leaf(None));
                assert_eq!(*far, Node::leaf(None));
            }
            other => panic!("expected a split, got {other:?}"),
        }
    }

    #[test]
    fn divide_turns_a_nested_named_leaf_into_a_split() {
        let edited = divide_ok(DIVIDE_SPEC, Some("root.far.near"), Some("x"), Some(40));
        let spec = crate::spec::parse(&edited).expect("edited spec must parse");

        let Node::Split { far, .. } = &spec.divisions else {
            panic!("root must stay a split");
        };
        let Node::Split { near, .. } = far.as_ref() else {
            panic!("root.far must stay a split");
        };
        let Node::Split {
            axis,
            at_mm,
            near,
            far,
        } = near.as_ref()
        else {
            panic!("root.far.near must become a split");
        };

        assert_eq!(*axis, Axis::X);
        assert_eq!(*at_mm, 40);
        assert_eq!(**near, Node::leaf(Some("Power")));
        assert_eq!(**far, Node::leaf(None));
    }

    #[test]
    fn divide_preserves_comments_and_spacing_outside_the_target() {
        let original = include_str!("../tests/fixtures/divide-comments.yaml");
        let edited = divide_ok(original, Some("root.near"), Some("x"), Some(40));
        let expected = original
            .replacen(
                "    axis: y          # near axis comment",
                "    axis: x          # near axis comment",
                1,
            )
            .replacen(
                "    at_mm: 60        # near position comment",
                "    at_mm: 40        # near position comment",
                1,
            );

        assert_eq!(edited, expected);
    }

    #[test]
    fn divide_preserves_comments_when_replacing_a_leaf() {
        let original = include_str!("../tests/fixtures/divide-comments-leaf.yaml");
        let edited = divide_ok(original, None, Some("x"), Some(18));

        for comment in [
            "# boxgen divide leaf fixture",
            "# footprint comment",
            "# height comment",
            "# division comment",
            "# leaf comment",
        ] {
            assert!(
                edited.contains(comment),
                "lost comment {comment:?}:\n{edited}"
            );
        }
    }

    #[test]
    fn divide_rejects_a_malformed_path() {
        for path in ["nonsense", "", "root.", "root.middle", "root.near.middle"] {
            let error = divide_error(DIVIDE_SPEC, Some(path), Some("x"), Some(40));
            let message = error.message();
            assert!(
                message.contains("path") || message.contains("segment"),
                "{path:?}: {message}"
            );
        }

        let error = divide_error(DIVIDE_SPEC, Some("root.middle"), None, Some(40));
        let message = error.message();
        assert!(message.contains("unknown segment `middle`"), "{message}");
    }

    #[test]
    fn divide_rejects_a_path_that_runs_through_a_leaf() {
        let error = divide_error(DIVIDE_SPEC, Some("root.near.far.near"), Some("x"), Some(40));

        assert!(matches!(error, DivideError::Invalid(_)), "{error:?}");
        let message = error.message();
        assert!(message.contains("does not resolve"), "{message}");
        assert!(message.contains("is a leaf"), "{message}");
    }

    #[test]
    fn divide_rejects_a_split_edit_without_flags() {
        let error = divide_error(DIVIDE_SPEC, Some("root.near"), None, None);

        assert!(matches!(error, DivideError::Usage(_)), "{error:?}");
        assert!(
            error.message().contains("provide --axis and/or --at"),
            "{}",
            error.message()
        );
    }

    #[test]
    fn divide_rejects_a_leaf_edit_without_both_flags() {
        for (axis, at_mm) in [(Some("x"), None), (None, Some(20))] {
            let error = divide_error(DIVIDE_LEAF_SPEC, None, axis, at_mm);

            assert!(matches!(error, DivideError::Usage(_)), "{error:?}");
            assert!(
                error.message().contains("provide both --axis and --at"),
                "{}",
                error.message()
            );
        }
    }

    #[test]
    fn divide_rejects_a_leaf_below_the_minimum() {
        let near = divide_error(DIVIDE_LEAF_SPEC, None, Some("y"), Some(8));
        assert!(matches!(near, DivideError::Invalid(_)), "{near:?}");
        assert!(
            near.message()
                .contains("at_mm 8 is below the 12 mm minimum"),
            "{}",
            near.message()
        );
        assert!(
            near.message().contains("leaf 8.0 mm deep"),
            "{}",
            near.message()
        );

        let far = divide_error(DIVIDE_LEAF_SPEC, None, Some("x"), Some(30));
        assert!(matches!(far, DivideError::Invalid(_)), "{far:?}");
        assert!(
            far.message().contains("leaf 5.5 mm wide"),
            "{}",
            far.message()
        );
    }

    #[test]
    fn divide_rejects_a_coincident_divider() {
        let original = include_str!("../tests/fixtures/divide-coincident.yaml");
        let error = divide_error(original, Some("root.near"), None, Some(80));

        assert!(matches!(error, DivideError::Invalid(_)), "{error:?}");
        assert!(
            error
                .message()
                .contains("coincides with an ancestor divider"),
            "{}",
            error.message()
        );
    }

    #[test]
    fn divide_rejects_a_tree_deeper_than_thirty_two() {
        let depth = usize::try_from(MAX_TREE_DEPTH).expect("depth fits usize");
        let text = format!(
            "system: SYS-2.0\nid: 4x4-6u-deep-v1\nfootprint: [4, 4]\nheight_u: 6\nlid: false\ndivisions:\n{}",
            deep_divisions(depth)
        );
        let path = format!("root{}", ".near".repeat(depth));

        let error = divide_error(&text, Some(&path), Some("x"), Some(12));

        assert!(matches!(error, DivideError::Invalid(_)), "{error:?}");
        assert!(
            error
                .message()
                .contains("tree depth 33 exceeds the 32 limit"),
            "{}",
            error.message()
        );
    }

    /// Builds a valid-shaped chain of `depth` x splits with `near` nested and
    /// a far leaf at every level, ending in a named leaf.
    fn deep_divisions(depth: usize) -> String {
        fn build(depth: usize, indent: usize) -> String {
            let pad = " ".repeat(indent);
            let mut out = format!("{pad}axis: x\n{pad}at_mm: 12\n{pad}near:\n");
            if depth == 1 {
                out.push_str(&format!("{}compartment: Deep\n", " ".repeat(indent + 2)));
            } else {
                out.push_str(&build(depth - 1, indent + 2));
            }
            out.push_str(&format!("{pad}far: {{}}\n"));
            out
        }

        build(depth, 2)
    }

    #[test]
    fn divide_output_revalidates() {
        let edited = divide_ok(DIVIDE_SPEC, Some("root.near"), Some("x"), Some(40));
        let spec = crate::spec::parse(&edited).expect("edited spec must parse");
        let validation = spec.validate();

        assert!(validation.is_valid(), "{:?}", validation.errors);
    }
}
