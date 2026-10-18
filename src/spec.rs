//! YAML box-spec schema, parsing, and validation.
//!
//! Contract: `docs/spec/docs/generator.md` §3 (input schema), §7 (validation rules), and
//! §9 (non-goals). The spec is the source of truth for one body; wall
//! thickness, radii, lip dimensions and print settings are fixed by the
//! standard and must never be expressible in a spec.
//!
//! Schema validation stays in this module: the other commands receive a parsed
//! spec, never a raw file path.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

use crate::error::BoxgenError;
use crate::tree::{self, Axis, BodyGeometry, Divider, Estimate, Leaf, Node, TreeReport, fmt_mm};

/// The only system version this compiler accepts (contract §3).
pub const SYSTEM_VERSION: &str = "SYS-1.0";

/// Rigid-item clearance added on each side for the contents fit check.
///
/// Standard §8: add 2 mm per side for rigid items and 5 mm for soft items and
/// cables. The check therefore needs `item width + 2 * clearance <= interior
/// width`.
pub const CONTENTS_CLEARANCE_PER_SIDE_MM: f64 = 2.0;

/// Soft-item clearance added on each side for the contents fit check.
///
/// Standard §8: 5 mm per side for soft items and cables (`contents.soft`).
pub const CONTENTS_SOFT_CLEARANCE_PER_SIDE_MM: f64 = 5.0;

/// Vertical clearance above a rigid item, standard §8.
pub const CONTENTS_HEIGHT_CLEARANCE_MM: f64 = 5.0;

/// Vertical clearance above a soft item or cable, standard §8.
pub const CONTENTS_SOFT_HEIGHT_CLEARANCE_MM: f64 = 10.0;

/// Headroom kept when an item lies diagonally, standard §8.
pub const CONTENTS_DIAGONAL_MARGIN_MM: f64 = 10.0;

/// Maximum contents weight per body, standard §4.5.
pub const MAX_CONTENTS_KG: f64 = 10.0;

/// Standard §8 clearances for one contents kind: `(per side, above)`.
pub fn contents_clearances(soft: bool) -> (f64, f64) {
    if soft {
        (
            CONTENTS_SOFT_CLEARANCE_PER_SIDE_MM,
            CONTENTS_SOFT_HEIGHT_CLEARANCE_MM,
        )
    } else {
        (CONTENTS_CLEARANCE_PER_SIDE_MM, CONTENTS_HEIGHT_CLEARANCE_MM)
    }
}

/// Standard §4.5: a missing weight is fine; the limit is inclusive.
pub fn weight_within_limit(weight_kg: Option<f64>) -> bool {
    weight_kg.is_none_or(|weight| weight <= MAX_CONTENTS_KG)
}

/// One item's standard §8 fit result against one body interior.
///
/// This is the one contents-fit predicate: [`Spec::validate`] warns from it
/// and the local UI's fit assistant answers from it, so a warning and a
/// viability answer can never disagree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContentsFit {
    /// Clearance applied on each side for this contents kind.
    pub per_side_mm: f64,
    /// Clearance applied above the item for this contents kind.
    pub height_clearance_mm: f64,
    /// Usable interior diagonal: the diagonal minus the diagonal margin.
    pub diagonal_limit_mm: f64,
    /// `max_length_mm` is absent or within the diagonal limit.
    pub length_ok: bool,
    /// `max_width_mm` is absent or fits with the side clearances.
    pub width_ok: bool,
    /// `max_height_mm` is absent or fits with the vertical clearance.
    pub height_ok: bool,
    /// `weight_kg` is absent or within the per-body limit.
    pub weight_ok: bool,
}

impl ContentsFit {
    /// True when every present check passes.
    pub fn fits(&self) -> bool {
        self.length_ok && self.width_ok && self.height_ok && self.weight_ok
    }
}

/// A parsed and structurally valid box spec.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    /// System version; semantic validation requires [`SYSTEM_VERSION`].
    pub system: String,
    /// Box ID, also the box folder name.
    pub id: String,
    /// Footprint `[N, M]` in Gridfinity units.
    pub footprint: [u32; 2],
    /// Height in Gridfinity units: 3, 6, 9 or 12.
    pub height_u: u32,
    /// Whether the build includes a lid.
    pub lid: bool,
    /// Whether the optional pair of 0.15 mm friction ribs is added to the lid
    /// skirt (standard §4.4/§6; only meaningful when `lid` is true).
    pub lid_ribs: bool,
    /// Free text for `notes.md`.
    pub notes: Option<String>,
    /// Optional contents metadata.
    pub contents: Option<Contents>,
    /// Root of the division tree.
    pub divisions: Node,
    /// Internal calibration-coupon flag (ticket #10).
    ///
    /// Never expressible in a user spec: [`RawSpec`] rejects a `coupon` field
    /// as unknown, so `parse` always yields `false`. `boxgen coupon` builds
    /// the one coupon spec in code and sets it, and the master adds the
    /// calibration T-divider when the embedded spec carries it.
    pub coupon: bool,
}

/// Optional contents metadata used for fit and weight warnings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Contents {
    /// Free-text description of the contents.
    pub description: Option<String>,
    /// Whether the contents are soft (cables, textiles). Soft items use the
    /// standard §8 clearances for soft items and cables: 5 mm per side and
    /// 10 mm above, instead of the rigid 2 mm / 5 mm.
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

/// A schema or structural parsing failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct SpecError {
    message: String,
}

impl SpecError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// The result of semantic validation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Validation {
    /// Rule violations; any entry makes the spec invalid.
    pub errors: Vec<String>,
    /// Advisory findings that do not fail the run.
    pub warnings: Vec<String>,
    /// Leaves in traversal order, used by the report.
    pub leaves: Vec<Leaf>,
    /// Divider plates in traversal order, positioned absolutely in the body
    /// interior (the layout the local UI plans and previews).
    pub dividers: Vec<Divider>,
    /// Documented mass and print-time estimate.
    pub estimate: Estimate,
}

impl Validation {
    /// True when no rule violation was found.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Parses YAML into a spec without applying semantic rules.
///
/// The schema is closed: unknown fields at the root, in the division tree, or
/// in `contents` are rejected so a spec cannot express wall thickness, radii,
/// print settings, or any other fixed architecture parameter.
pub fn parse(text: &str) -> Result<Spec, SpecError> {
    let raw: RawSpec = serde_yaml_ng::from_str(text)
        .map_err(|error| SpecError::new(format!("YAML parse error: {error}")))?;

    let footprint_len = raw.footprint.len();
    let footprint: [u32; 2] = raw.footprint.try_into().map_err(|_| {
        SpecError::new(format!(
            "footprint: expected exactly two integers [N, M], got {footprint_len}"
        ))
    })?;
    let divisions = raw.divisions.into_node("divisions")?;

    Ok(Spec {
        system: raw.system,
        id: raw.id,
        footprint,
        height_u: raw.height_u,
        lid: raw.lid,
        lid_ribs: raw.lid_ribs.unwrap_or(false),
        notes: raw.notes,
        contents: raw.contents.map(Contents::from),
        divisions,
        coupon: false,
    })
}

/// Reads and parses a spec file, mapping every failure to the exit-code-2
/// invalid-spec error.
pub fn load(path: &Path) -> Result<Spec, BoxgenError> {
    let text = fs::read_to_string(path).map_err(|error| BoxgenError::InvalidSpec {
        path: path.to_path_buf(),
        message: format!("cannot read spec: {error}"),
    })?;

    parse(&text).map_err(|error| BoxgenError::InvalidSpec {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

/// Maps collected rule violations to the contract's exit-code-2 error.
pub(crate) fn invalid_spec(path: &Path, errors: &[String]) -> BoxgenError {
    let count = errors.len();
    let noun = if count == 1 {
        "validation error"
    } else {
        "validation errors"
    };
    BoxgenError::InvalidSpec {
        path: path.to_path_buf(),
        message: format!("{count} {noun}:\n  - {}", errors.join("\n  - ")),
    }
}

/// Parses and validates `spec`, printing a human-readable report.
pub fn validate(path: &Path) -> Result<(), BoxgenError> {
    let spec = load(path)?;
    let validation = spec.validate();

    if !validation.is_valid() {
        return Err(invalid_spec(path, &validation.errors));
    }

    print!("{}", spec.report(&validation));
    Ok(())
}

impl Contents {
    /// Applies the standard §8 fit rules against one body interior.
    pub fn fit(&self, body: &BodyGeometry) -> ContentsFit {
        let (per_side_mm, height_clearance_mm) = contents_clearances(self.soft);
        let diagonal_limit_mm =
            body.interior_width_mm.hypot(body.interior_depth_mm) - CONTENTS_DIAGONAL_MARGIN_MM;

        ContentsFit {
            per_side_mm,
            height_clearance_mm,
            diagonal_limit_mm,
            length_ok: self
                .max_length_mm
                .is_none_or(|length| length <= diagonal_limit_mm),
            width_ok: self
                .max_width_mm
                .is_none_or(|width| width + 2.0 * per_side_mm <= body.interior_width_mm),
            height_ok: self
                .max_height_mm
                .is_none_or(|height| height + height_clearance_mm <= body.interior_height_mm),
            weight_ok: weight_within_limit(self.weight_kg),
        }
    }
}

impl Spec {
    /// Resolves the fixed body geometry for this spec.
    pub fn body(&self) -> BodyGeometry {
        BodyGeometry::new(self.footprint, self.height_u)
    }

    /// Renders the canonical `spec.yaml` for this spec.
    ///
    /// This is the one writer for specs the local UI saves (`boxgen serve`,
    /// ticket #17): the output is deterministic, re-parses to an equal
    /// [`Spec`], and drops comments and layout — the CLI's `divide` stays the
    /// lossless editor. Fixed architecture values are never emitted, and the
    /// internal `coupon` flag stays internal.
    pub fn to_yaml(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "system: {}", self.system);
        let _ = writeln!(out, "id: {}", self.id);
        let _ = writeln!(
            out,
            "footprint: [{}, {}]",
            self.footprint[0], self.footprint[1]
        );
        let _ = writeln!(out, "height_u: {}", self.height_u);
        let _ = writeln!(out, "lid: {}", self.lid);
        if self.lid_ribs {
            let _ = writeln!(out, "lid_ribs: true");
        }
        if let Some(notes) = &self.notes {
            let _ = writeln!(out, "notes: {}", yaml_string(notes));
        }
        match self.contents.as_ref().map(contents_yaml) {
            Some(lines) if !lines.is_empty() => {
                let _ = writeln!(out, "contents:");
                for line in lines {
                    let _ = writeln!(out, "  {line}");
                }
            }
            Some(_) => {
                let _ = writeln!(out, "contents: {{}}");
            }
            None => {}
        }
        let _ = write!(out, "divisions:");
        write_node(&mut out, &self.divisions, 0);
        out
    }

    /// Applies every schema and contract validation rule.
    ///
    /// Geometry-dependent rules are skipped when the footprint or height is
    /// outside the schema range, because no meaningful body exists to check.
    pub fn validate(&self) -> Validation {
        let mut validation = Validation::default();
        self.validate_schema(&mut validation);

        let in_range = tree::MIN_FOOTPRINT_UNITS..=tree::MAX_FOOTPRINT_UNITS;
        let footprint_valid = self.footprint.iter().all(|side| in_range.contains(side));
        let height_valid = tree::HEIGHTS_U.contains(&self.height_u);
        if !(footprint_valid && height_valid) {
            return validation;
        }

        let body = self.body();
        validation.errors.extend(body.printer_limit_violations());

        let TreeReport {
            leaves,
            dividers,
            errors,
            warnings,
            divider_volume_mm3,
        } = tree::validate_tree(&body, &self.divisions);
        validation.errors.extend(errors);
        validation.warnings.extend(warnings);
        validation.leaves = leaves;
        validation.dividers = dividers;
        validation.estimate = body.estimate(divider_volume_mm3, self.lid);

        if let Some(contents) = &self.contents {
            self.validate_contents(&body, contents, &mut validation);
        }
        if validation.estimate.mass_g > tree::MAX_MASS_G {
            validation.warnings.push(format!(
                "estimated mass {:.0} g exceeds the {:.0} g warning threshold",
                validation.estimate.mass_g,
                tree::MAX_MASS_G
            ));
        }
        if validation.estimate.print_hours > tree::MAX_PRINT_HOURS {
            validation.warnings.push(format!(
                "estimated print time {:.1} h exceeds the {:.0} h warning threshold",
                validation.estimate.print_hours,
                tree::MAX_PRINT_HOURS
            ));
        }

        validation
    }

    /// Renders the human-readable report for a valid spec.
    pub fn report(&self, validation: &Validation) -> String {
        let body = self.body();
        let mut out = String::new();

        let _ = writeln!(out, "id: {}", self.id);
        let _ = writeln!(out, "system: {}", self.system);
        let _ = writeln!(
            out,
            "footprint: {} x {} (outer {} x {} mm)",
            self.footprint[0],
            self.footprint[1],
            fmt_mm(body.outer_width_mm),
            fmt_mm(body.outer_depth_mm)
        );
        let _ = writeln!(
            out,
            "height: {}U ({} mm total)",
            self.height_u,
            fmt_mm(body.total_height_mm)
        );
        let _ = writeln!(
            out,
            "interior: {} x {} x {} mm",
            fmt_mm(body.interior_width_mm),
            fmt_mm(body.interior_depth_mm),
            fmt_mm(body.interior_height_mm)
        );
        let _ = writeln!(out, "lid: {}", if self.lid { "yes" } else { "no" });
        let _ = writeln!(out, "compartments: {}", validation.leaves.len());
        for leaf in &validation.leaves {
            let name = leaf.compartment.as_deref().unwrap_or("(unnamed)");
            let _ = writeln!(
                out,
                "  - {name} ({} x {} mm)",
                fmt_mm(leaf.width_mm),
                fmt_mm(leaf.depth_mm)
            );
        }

        let estimate_parts = if self.lid {
            "shell + dividers + lid"
        } else {
            "shell + dividers"
        };
        let _ = writeln!(
            out,
            "estimated mass: {:.0} g ({estimate_parts}; PETG {:.2} g/cm3)",
            validation.estimate.mass_g,
            tree::PETG_DENSITY_G_PER_CM3
        );
        let _ = writeln!(
            out,
            "estimated print time: {:.1} h (at {:.1} mm3/s effective flow)",
            validation.estimate.print_hours,
            tree::EFFECTIVE_FLOW_MM3_PER_S
        );

        if validation.warnings.is_empty() {
            let _ = writeln!(out, "warnings: none");
        } else {
            let _ = writeln!(out, "warnings:");
            for warning in &validation.warnings {
                let _ = writeln!(out, "  - {warning}");
            }
        }

        out
    }

    fn validate_schema(&self, validation: &mut Validation) {
        if self.system != SYSTEM_VERSION {
            validation.errors.push(format!(
                "system: must be {SYSTEM_VERSION:?}, got {:?}",
                self.system
            ));
        }

        if !is_valid_id(&self.id) {
            validation.errors.push(format!(
                "id: {:?} must be lowercase kebab-case or the standard <N>x<M>-<height>U-<slug>-<rev> form (for example 4x4-6U-usb-cables-v1)",
                self.id
            ));
        }

        for (index, side) in self.footprint.iter().enumerate() {
            let in_range = tree::MIN_FOOTPRINT_UNITS..=tree::MAX_FOOTPRINT_UNITS;
            if !in_range.contains(side) {
                let (min, max) = (tree::MIN_FOOTPRINT_UNITS, tree::MAX_FOOTPRINT_UNITS);
                validation.errors.push(format!(
                    "footprint[{index}]: {side} is outside {min}..={max}"
                ));
            }
        }

        if !tree::HEIGHTS_U.contains(&self.height_u) {
            let heights = tree::HEIGHTS_U.map(|height| height.to_string()).join(", ");
            validation.errors.push(format!(
                "height_u: {} is not one of {heights}",
                self.height_u
            ));
        }

        if self.lid_ribs && !self.lid {
            validation.errors.push(
                "lid_ribs: the friction ribs need a lid; set `lid: true` or `lid_ribs: false`"
                    .to_owned(),
            );
        }
    }

    /// Contents checks follow standard §8 sizing rules.
    ///
    /// - Long items may lie diagonally: `length <= interior diagonal - 10 mm`.
    /// - Rigid items need 2 mm per side and 5 mm above; soft items and cables
    ///   (`contents.soft: true`) need 5 mm per side and 10 mm above.
    /// - The standard's 10 mm "where fingers must reach" rule stays a human
    ///   sizing rule: it depends on how the box is used, not on the contents
    ///   geometry, so the schema deliberately has no field for it.
    ///
    /// The check uses the whole body interior, not the largest leaf; the
    /// contents record describes the box, not a particular compartment.
    fn validate_contents(
        &self,
        body: &BodyGeometry,
        contents: &Contents,
        validation: &mut Validation,
    ) {
        let fit = contents.fit(body);

        if let Some(length) = contents.max_length_mm
            && !fit.length_ok
        {
            let diagonal = body.interior_width_mm.hypot(body.interior_depth_mm);
            validation.warnings.push(format!(
                "contents.max_length_mm: {length:.1} mm exceeds the {:.1} mm usable diagonal (interior diagonal {diagonal:.1} mm minus {CONTENTS_DIAGONAL_MARGIN_MM:.0} mm)",
                fit.diagonal_limit_mm
            ));
        }

        if let Some(width) = contents.max_width_mm
            && !fit.width_ok
        {
            let per_side = fit.per_side_mm;
            let required = width + 2.0 * per_side;
            validation.warnings.push(format!(
                "contents.max_width_mm: {width:.1} mm plus {per_side:.0} mm per side needs {required:.1} mm, more than the {:.1} mm interior width",
                body.interior_width_mm
            ));
        }

        if let Some(height) = contents.max_height_mm
            && !fit.height_ok
        {
            let height_clearance = fit.height_clearance_mm;
            let required = height + height_clearance;
            validation.warnings.push(format!(
                "contents.max_height_mm: {height:.1} mm plus {height_clearance:.0} mm clearance needs {required:.1} mm, more than the {:.1} mm interior depth",
                body.interior_height_mm
            ));
        }

        if let Some(weight) = contents.weight_kg
            && !fit.weight_ok
        {
            validation.warnings.push(format!(
                "contents.weight_kg: {weight:.1} kg exceeds the {MAX_CONTENTS_KG:.0} kg per-body limit"
            ));
        }
    }
}

/// Quotes a string as a YAML double-quoted scalar.
///
/// JSON string escaping is a subset of YAML double-quoted escaping, so
/// `serde_json` supplies the quoting for free; notes and compartment names
/// are free text.
fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).expect("a string always serializes")
}

/// Formats a number so the canonical form is visibly a float (`25` would
/// parse as an integer, which is legal but not the canonical spelling).
fn yaml_number(value: f64) -> String {
    let text = format!("{value}");
    if text.contains('.') || text.contains('e') || text.contains('E') {
        text
    } else {
        format!("{text}.0")
    }
}

/// Renders the `contents` block's lines; the caller indents them.
fn contents_yaml(contents: &Contents) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(description) = &contents.description {
        lines.push(format!("description: {}", yaml_string(description)));
    }
    if contents.soft {
        lines.push("soft: true".to_owned());
    }
    for (key, value) in [
        ("max_length_mm", contents.max_length_mm),
        ("max_width_mm", contents.max_width_mm),
        ("max_height_mm", contents.max_height_mm),
        ("weight_kg", contents.weight_kg),
    ] {
        if let Some(value) = value {
            lines.push(format!("{key}: {}", yaml_number(value)));
        }
    }
    lines
}

/// Appends one division node to `out`, whose prefix (`divisions:`, `near:`)
/// has already been written.
///
/// A leaf continues the caller's line as a flow mapping; a split starts a
/// block two spaces deeper. This keeps the canonical text free of trailing
/// whitespace.
fn write_node(out: &mut String, node: &Node, indent: usize) {
    match node {
        Node::Leaf { compartment } => {
            out.push(' ');
            match compartment {
                Some(name) => {
                    let _ = write!(out, "{{compartment: {}}}", yaml_string(name));
                }
                None => out.push_str("{}"),
            }
            out.push('\n');
        }
        Node::Split {
            axis,
            at_mm,
            near,
            far,
        } => {
            let pad = " ".repeat(indent + 2);
            out.push('\n');
            let _ = writeln!(out, "{pad}axis: {}", axis.as_str());
            let _ = writeln!(out, "{pad}at_mm: {at_mm}");
            let _ = write!(out, "{pad}near:");
            write_node(out, near, indent + 2);
            let _ = write!(out, "{pad}far:");
            write_node(out, far, indent + 2);
        }
    }
}

/// Checks the box ID rule, accepting both the contract and standard forms.
///
/// The generator contract §3 states the generic lowercase kebab-case rule
/// `^[a-z0-9]+(-[a-z0-9]+)*$`, but the frozen SYS-1.0 standard §10 names boxes
/// `<footprint>-<height>-<slug>-<rev>` with an uppercase height unit, for
/// example `4x4-6U-usb-cables-v1`. The standard wins, so both forms are valid:
///
/// - generic: `^[a-z0-9]+(-[a-z0-9]+)*$`
/// - standard: `^[0-9]+x[0-9]+-(3|6|9|12)U-[a-z0-9]+(-[a-z0-9]+)*$`
///
/// Arbitrary uppercase, empty segments, and path separators remain invalid.
pub fn is_valid_id(id: &str) -> bool {
    is_lowercase_kebab(id) || is_standard_box_id(id)
}

fn is_lowercase_kebab(id: &str) -> bool {
    id.split('-').all(is_lowercase_alphanumeric_segment)
}

fn is_standard_box_id(id: &str) -> bool {
    let mut parts = id.split('-');
    let Some(footprint) = parts.next() else {
        return false;
    };
    let Some(height) = parts.next() else {
        return false;
    };
    let slug = parts.collect::<Vec<_>>();

    is_standard_footprint(footprint)
        && matches!(height, "3U" | "6U" | "9U" | "12U")
        && !slug.is_empty()
        && slug.into_iter().all(is_lowercase_alphanumeric_segment)
}

fn is_standard_footprint(footprint: &str) -> bool {
    let Some((width, depth)) = footprint.split_once('x') else {
        return false;
    };

    !width.is_empty()
        && !depth.is_empty()
        && !depth.contains('x')
        && width.bytes().all(|byte| byte.is_ascii_digit())
        && depth.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_lowercase_alphanumeric_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSpec {
    system: String,
    id: String,
    footprint: Vec<u32>,
    height_u: u32,
    lid: bool,
    lid_ribs: Option<bool>,
    notes: Option<String>,
    contents: Option<RawContents>,
    divisions: RawNode,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawContents {
    description: Option<String>,
    soft: Option<bool>,
    max_length_mm: Option<f64>,
    max_width_mm: Option<f64>,
    max_height_mm: Option<f64>,
    weight_kg: Option<f64>,
}

impl From<RawContents> for Contents {
    fn from(raw: RawContents) -> Self {
        Self {
            description: raw.description,
            soft: raw.soft.unwrap_or(false),
            max_length_mm: raw.max_length_mm,
            max_width_mm: raw.max_width_mm,
            max_height_mm: raw.max_height_mm,
            weight_kg: raw.weight_kg,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNode {
    compartment: Option<String>,
    axis: Option<Axis>,
    at_mm: Option<u32>,
    near: Option<Box<RawNode>>,
    far: Option<Box<RawNode>>,
}

impl RawNode {
    fn into_node(self, path: &str) -> Result<Node, SpecError> {
        let split_fields = [
            ("axis", self.axis.is_some()),
            ("at_mm", self.at_mm.is_some()),
            ("near", self.near.is_some()),
            ("far", self.far.is_some()),
        ];
        let has_split_field = split_fields.iter().any(|(_, present)| *present);

        if !has_split_field {
            return Ok(Node::Leaf {
                compartment: self.compartment,
            });
        }

        if self.compartment.is_some() {
            return Err(SpecError::new(format!(
                "{path}: a node cannot mix `compartment` with split fields `axis`, `at_mm`, `near`, `far`"
            )));
        }

        let missing = split_fields
            .iter()
            .filter_map(|(name, present)| (!present).then_some(*name))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(SpecError::new(format!(
                "{path}: split is missing `{}`",
                missing.join("`, `")
            )));
        }

        let (axis, at_mm, near, far) = match (self.axis, self.at_mm, self.near, self.far) {
            (Some(axis), Some(at_mm), Some(near), Some(far)) => (axis, at_mm, near, far),
            _ => {
                return Err(SpecError::new(format!(
                    "{path}: split is missing one of `axis`, `at_mm`, `near`, `far`"
                )));
            }
        };

        Ok(Node::Split {
            axis,
            at_mm,
            near: Box::new(near.into_node(&format!("{path}.near"))?),
            far: Box::new(far.into_node(&format!("{path}.far"))?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = "\
system: SYS-1.0
id: 1x1-3u-bits-v1
footprint: [1, 1]
height_u: 3
lid: false
divisions: { compartment: Bits }
";

    fn parse_ok(yaml: &str) -> Spec {
        parse(yaml).unwrap_or_else(|error| panic!("expected valid spec, got {error}"))
    }

    fn validation(yaml: &str) -> Validation {
        parse_ok(yaml).validate()
    }

    fn errors(yaml: &str) -> Vec<String> {
        validation(yaml).errors
    }

    fn warnings(yaml: &str) -> Vec<String> {
        validation(yaml).warnings
    }

    fn assert_error(yaml: &str, needle: &str) {
        let errors = errors(yaml);
        assert!(
            errors.iter().any(|error| error.contains(needle)),
            "expected an error containing {needle:?}, got {errors:?}"
        );
    }

    fn with_divisions(root: &str) -> String {
        let header = MINIMAL
            .lines()
            .filter(|line| !line.starts_with("divisions:"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("{header}\ndivisions:\n{root}")
    }

    #[test]
    fn minimal_spec_parses_and_validates() {
        let spec = parse_ok(MINIMAL);
        let validation = spec.validate();

        assert_eq!(spec.system, SYSTEM_VERSION);
        assert_eq!(spec.id, "1x1-3u-bits-v1");
        assert_eq!(spec.footprint, [1, 1]);
        assert_eq!(spec.height_u, 3);
        assert!(!spec.lid);
        assert!(validation.is_valid(), "{:?}", validation.errors);
        assert!(validation.warnings.is_empty(), "{:?}", validation.warnings);
        assert_eq!(validation.leaves.len(), 1);
    }

    #[test]
    fn committed_spec_template_is_valid() {
        let spec = parse(include_str!("../templates/box-spec.yaml")).unwrap();
        let validation = spec.validate();

        assert!(validation.is_valid(), "{:?}", validation.errors);
        assert_eq!(validation.leaves.len(), 4);
        assert!(
            validation.warnings.is_empty(),
            "the committed example must be warning-free: {:?}",
            validation.warnings
        );
        assert_eq!(spec.id, "4x4-6U-example-v1");
    }

    #[test]
    fn spec_repository_template_validates_when_present() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/spec/templates/box-spec.yaml");
        if !path.is_file() {
            eprintln!(
                "skipping spec template check: {} does not exist",
                path.display()
            );
            return;
        }

        let text = std::fs::read_to_string(&path).expect("vendored template must be readable");
        let spec = parse(&text).expect("vendored template must parse");
        let validation = spec.validate();

        assert!(
            validation.is_valid(),
            "{} must validate: {:?}",
            path.display(),
            validation.errors
        );
    }

    /// The spec's copy of the template and the crate's copy are one skeleton:
    /// byte-identical, so edits to one cannot leave the other behind.
    #[test]
    fn spec_repository_template_matches_the_crate_copy() {
        let spec_template =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/spec/templates/box-spec.yaml");
        if !spec_template.is_file() {
            eprintln!(
                "skipping template comparison: {} does not exist",
                spec_template.display()
            );
            return;
        }
        let crate_template = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/box-spec.yaml");

        let crate_text = std::fs::read_to_string(&crate_template).expect("crate template");
        let spec_text = std::fs::read_to_string(&spec_template).expect("spec template");
        assert_eq!(
            crate_text, spec_text,
            "templates/box-spec.yaml must match docs/spec/templates/box-spec.yaml"
        );
    }

    #[test]
    fn system_must_be_sys_1_0() {
        let yaml = MINIMAL.replace("system: SYS-1.0", "system: SYS-9.9");
        assert_error(&yaml, "system");
        assert!(errors(&yaml).iter().any(|error| error.contains("SYS-1.0")));
    }

    #[test]
    fn ids_accept_the_generic_and_standard_forms() {
        for id in [
            "4x4-6U-usb-cables-v1",
            "4x4-6u-usb-cables-v1",
            "4x4-6U-example-v1",
            "1x1-3U-bits-v1",
            "4x4-12U-bulky-v1",
            "a",
            "1",
            "a-b-c-1",
        ] {
            assert!(is_valid_id(id), "{id} must be accepted");

            let yaml = MINIMAL.replace("1x1-3u-bits-v1", id);
            assert!(
                !errors(&yaml).iter().any(|error| error.starts_with("id:")),
                "{id} should be accepted: {:?}",
                errors(&yaml)
            );
        }
    }

    #[test]
    fn invalid_ids_are_rejected() {
        for id in [
            "4x4-6U-USB-v1",
            "4x4-6U-",
            "-4x4-6U-v1",
            "4x4-6U-a--b-v1",
            "4x4-6U-a/b-v1",
            "4x4-6U-a\\b-v1",
            "4x4-6X-usb-v1",
            "4x4-6U-usb_cables-v1",
            "Uppercase",
            "under_score",
            "double--dash",
            "trailing-",
            "has space",
        ] {
            assert!(!is_valid_id(id), "{id} must be rejected");

            let yaml = MINIMAL.replace("1x1-3u-bits-v1", id);
            assert_error(&yaml, "id");
        }
    }

    #[test]
    fn footprint_sides_are_between_one_and_four() {
        for footprint in ["[0, 1]", "[5, 4]", "[4, 9]"] {
            let yaml = MINIMAL.replace("footprint: [1, 1]", &format!("footprint: {footprint}"));
            assert_error(&yaml, "footprint");
        }

        let yaml = MINIMAL.replace("footprint: [1, 1]", "footprint: [4, 1]");
        assert!(validation(&yaml).is_valid());
    }

    #[test]
    fn footprint_needs_exactly_two_integers() {
        for footprint in ["[4]", "[4, 4, 4]"] {
            let yaml = MINIMAL.replace("footprint: [1, 1]", &format!("footprint: {footprint}"));
            let error = parse(&yaml).unwrap_err().to_string();
            assert!(error.contains("exactly two"), "{error}");
        }

        for footprint in ["[4, 4.5]", "[-1, 4]", "4"] {
            let yaml = MINIMAL.replace("footprint: [1, 1]", &format!("footprint: {footprint}"));
            assert!(parse(&yaml).is_err(), "{footprint} must not parse");
        }
    }

    #[test]
    fn height_is_one_of_three_six_nine_twelve() {
        for height in [3, 6, 9, 12] {
            let yaml = MINIMAL.replace("height_u: 3", &format!("height_u: {height}"));
            assert!(validation(&yaml).is_valid(), "{height}U must be valid");
        }

        for height in [0, 1, 7, 15] {
            let yaml = MINIMAL.replace("height_u: 3", &format!("height_u: {height}"));
            assert_error(&yaml, "height_u");
        }
    }

    #[test]
    fn lid_must_be_a_boolean() {
        for lid in ["\"true\"", "1", "yes"] {
            let yaml = MINIMAL.replace("lid: false", &format!("lid: {lid}"));
            let error = parse(&yaml)
                .expect_err("a non-boolean lid must not parse")
                .to_string();
            assert!(error.contains("boolean"), "{error}");
        }

        let yaml = MINIMAL.replace("lid: false", "lid: true");
        assert!(parse_ok(&yaml).lid);
    }

    #[test]
    fn lid_ribs_defaults_to_false_and_must_be_a_boolean() {
        assert!(!parse_ok(MINIMAL).lid_ribs);

        for lid_ribs in ["\"true\"", "1", "yes"] {
            let yaml = format!("{MINIMAL}lid_ribs: {lid_ribs}\n");
            let error = parse(&yaml)
                .expect_err("a non-boolean lid_ribs must not parse")
                .to_string();
            assert!(error.contains("boolean"), "{error}");
        }

        let yaml = format!("{MINIMAL}lid_ribs: true\n");
        // Without a lid the ribs are meaningless; the value parses, the
        // combination is rejected by validation.
        assert!(parse_ok(&yaml).lid_ribs);
    }

    #[test]
    fn lid_ribs_require_a_lid() {
        let yaml = MINIMAL.replace("lid: false", "lid: false\nlid_ribs: true");
        let ribs_without_lid = errors(&yaml);

        assert!(
            ribs_without_lid
                .iter()
                .any(|error| error.contains("lid_ribs") && error.contains("lid")),
            "the error must name both fields: {ribs_without_lid:?}"
        );

        let with_lid = yaml.replace("lid: false", "lid: true");
        assert!(
            validation(&with_lid).is_valid(),
            "lid_ribs with a lid must be valid: {:?}",
            errors(&with_lid)
        );
    }

    #[test]
    fn required_fields_are_required() {
        for line in [
            "system: SYS-1.0\n",
            "id: 1x1-3u-bits-v1\n",
            "footprint: [1, 1]\n",
            "height_u: 3\n",
            "lid: false\n",
            "divisions: { compartment: Bits }\n",
        ] {
            let yaml = MINIMAL.replace(line, "");
            let error = parse(&yaml)
                .expect_err("missing required field must not parse")
                .to_string();
            assert!(error.contains("missing field"), "{error}");
        }
    }

    #[test]
    fn forbidden_top_level_fields_are_rejected() {
        for field in [
            "wall_thickness: 2.0",
            "corner_radius: 3.75",
            "print_speed: 100",
            "layer_height: 0.2",
        ] {
            let yaml = format!("{MINIMAL}{field}\n");
            let error = parse(&yaml)
                .expect_err("forbidden architecture field must not parse")
                .to_string();
            assert!(error.contains("unknown field"), "{error}");
            assert!(error.contains(field.split(':').next().unwrap()), "{error}");
        }
    }

    /// The coupon flag is internal: `boxgen coupon` builds the spec in code,
    /// and the closed user schema rejects the field like any other unknown
    /// one. A parsed user spec is therefore never a coupon.
    #[test]
    fn the_coupon_flag_is_internal_and_rejected_in_a_user_spec() {
        let yaml = format!("{MINIMAL}coupon: true\n");
        let error = parse(&yaml)
            .expect_err("a user spec must not express the internal coupon flag")
            .to_string();

        assert!(error.contains("unknown field"), "{error}");
        assert!(error.contains("coupon"), "{error}");
        assert!(!parse_ok(MINIMAL).coupon, "a parsed spec is never a coupon");
    }

    #[test]
    fn unknown_fields_inside_divisions_are_rejected() {
        let yaml = with_divisions("  compartment: Bits\n  radius: 1.75\n");
        let error = parse(&yaml).unwrap_err().to_string();

        assert!(error.contains("unknown field"), "{error}");
        assert!(error.contains("radius"), "{error}");
    }

    #[test]
    fn unknown_fields_inside_contents_are_rejected() {
        let yaml = format!("{MINIMAL}contents:\n  print_speed: 100\n");
        let error = parse(&yaml).unwrap_err().to_string();

        assert!(error.contains("unknown field"), "{error}");
        assert!(error.contains("print_speed"), "{error}");
    }

    #[test]
    fn a_node_cannot_mix_leaf_and_split_fields() {
        let yaml = with_divisions("  compartment: A\n  axis: x\n");
        let error = parse(&yaml).unwrap_err().to_string();

        assert!(error.contains("compartment"), "{error}");
        assert!(error.contains("axis"), "{error}");
    }

    #[test]
    fn a_split_requires_all_of_its_fields() {
        let yaml = with_divisions("  axis: x\n  at_mm: 20\n");
        let error = parse(&yaml).unwrap_err().to_string();

        assert!(error.contains("missing"), "{error}");
        assert!(error.contains("near"), "{error}");
        assert!(error.contains("far"), "{error}");
    }

    #[test]
    fn negative_or_fractional_at_mm_is_a_parse_error() {
        for at_mm in ["-1", "12.5"] {
            let yaml = with_divisions(&format!(
                "  axis: x\n  at_mm: {at_mm}\n  near: {{ compartment: A }}\n  far: {{ compartment: B }}\n"
            ));
            assert!(parse(&yaml).is_err(), "at_mm {at_mm} must not parse");
        }
    }

    #[test]
    fn unknown_axes_are_a_parse_error() {
        let yaml = with_divisions(
            "  axis: z\n  at_mm: 20\n  near: { compartment: A }\n  far: { compartment: B }\n",
        );
        let error = parse(&yaml).unwrap_err().to_string();

        assert!(error.contains("unknown variant"), "{error}");
        assert!(error.contains('z'), "{error}");
    }

    #[test]
    fn contents_accept_integer_and_fractional_numbers() {
        let yaml = format!(
            "{MINIMAL}contents:\n  description: Cables\n  soft: true\n  max_length_mm: 200\n  max_width_mm: 80.5\n  max_height_mm: 25\n  weight_kg: 1.2\n"
        );
        let contents = parse_ok(&yaml).contents.expect("contents must parse");

        assert_eq!(contents.description.as_deref(), Some("Cables"));
        assert!(contents.soft, "soft must parse");
        assert_eq!(contents.max_length_mm, Some(200.0));
        assert_eq!(contents.max_width_mm, Some(80.5));
        assert_eq!(contents.max_height_mm, Some(25.0));
        assert_eq!(contents.weight_kg, Some(1.2));
    }

    /// `soft` defaults to false (rigid clearances) and must be a boolean when
    /// present; the closed contents schema still rejects unknown fields.
    #[test]
    fn contents_soft_defaults_to_false_and_must_be_a_boolean() {
        assert!(
            !parse_ok(&format!("{MINIMAL}contents:\n  description: Bits\n"))
                .contents
                .expect("contents must parse")
                .soft
        );

        for soft in ["\"true\"", "1", "yes"] {
            let yaml = format!("{MINIMAL}contents:\n  soft: {soft}\n");
            let error = parse(&yaml)
                .expect_err("a non-boolean soft must not parse")
                .to_string();
            assert!(error.contains("boolean"), "{error}");
        }

        let yaml = format!("{MINIMAL}contents:\n  soft: true\n");
        assert!(parse_ok(&yaml).contents.expect("contents must parse").soft);
    }

    /// Standard §8: soft items and cables get 5 mm per side and 10 mm above,
    /// where rigid items get 2 mm per side and 5 mm above.
    #[test]
    fn soft_contents_get_the_soft_clearances() {
        // 1x1 3U: interior width 37.5 mm, interior height 12.25 mm. The same
        // item passes the rigid rules and fails the soft ones.
        let rigid = format!("{MINIMAL}contents:\n  max_width_mm: 30\n  max_height_mm: 7\n");
        assert!(
            warnings(&rigid).is_empty(),
            "rigid boundary values must pass: {:?}",
            warnings(&rigid)
        );

        let soft =
            format!("{MINIMAL}contents:\n  soft: true\n  max_width_mm: 30\n  max_height_mm: 7\n");
        let soft_warnings = warnings(&soft);
        assert!(
            soft_warnings
                .iter()
                .any(|warning| warning.contains("contents.max_width_mm")
                    && warning.contains("5 mm per side")),
            "missing soft per-side warning: {soft_warnings:?}"
        );
        assert!(
            soft_warnings
                .iter()
                .any(|warning| warning.contains("contents.max_height_mm")
                    && warning.contains("10 mm clearance")),
            "missing soft height warning: {soft_warnings:?}"
        );

        // The soft boundary values fit: width + 2 * 5 <= 37.5 and
        // height + 10 <= 12.25.
        let fitting =
            format!("{MINIMAL}contents:\n  soft: true\n  max_width_mm: 27\n  max_height_mm: 2\n");
        assert!(
            warnings(&fitting).is_empty(),
            "soft boundary values must pass: {:?}",
            warnings(&fitting)
        );
    }

    #[test]
    fn contents_fit_failures_are_warnings_not_errors() {
        let yaml = format!(
            "{MINIMAL}contents:\n  max_length_mm: 50\n  max_width_mm: 34\n  max_height_mm: 8\n  weight_kg: 11\n"
        );
        let validation = validation(&yaml);

        assert!(validation.is_valid(), "{:?}", validation.errors);
        for field in [
            "contents.max_length_mm",
            "contents.max_width_mm",
            "contents.max_height_mm",
            "contents.weight_kg",
        ] {
            assert!(
                validation
                    .warnings
                    .iter()
                    .any(|warning| warning.contains(field)),
                "missing warning for {field}: {:?}",
                validation.warnings
            );
        }
    }

    #[test]
    fn contents_within_the_limits_do_not_warn() {
        let yaml = format!(
            "{MINIMAL}contents:\n  max_length_mm: 43\n  max_width_mm: 33\n  max_height_mm: 7\n  weight_kg: 10\n"
        );

        assert!(
            warnings(&yaml).is_empty(),
            "boundary values must pass: {:?}",
            warnings(&yaml)
        );
    }

    /// The one fit predicate reports each standard §8 check independently;
    /// the validator's warnings and the UI fit assistant both read it.
    #[test]
    fn contents_fit_reports_every_check_independently() {
        let body = BodyGeometry::new([1, 1], 3);
        let contents = Contents {
            description: None,
            soft: true,
            max_length_mm: Some(60.0),
            max_width_mm: Some(30.0),
            max_height_mm: Some(7.0),
            weight_kg: Some(11.0),
        };
        let fit = contents.fit(&body);

        assert!(!fit.fits());
        assert!(!fit.length_ok, "60 mm exceeds the 43.0 mm usable diagonal");
        assert!(!fit.width_ok, "30 + 10 mm exceeds the 37.5 mm width");
        assert!(!fit.height_ok, "7 + 10 mm exceeds the 12.25 mm height");
        assert!(!fit.weight_ok, "11 kg exceeds the 10 kg limit");
        assert_eq!(fit.per_side_mm, CONTENTS_SOFT_CLEARANCE_PER_SIDE_MM);
        assert_eq!(fit.height_clearance_mm, CONTENTS_SOFT_HEIGHT_CLEARANCE_MM);
        assert_eq!(
            fit.diagonal_limit_mm,
            body.interior_width_mm.hypot(body.interior_depth_mm) - CONTENTS_DIAGONAL_MARGIN_MM
        );

        let empty = Contents {
            soft: true,
            ..Contents::default()
        };
        assert!(empty.fit(&body).fits(), "absent checks pass");
    }

    #[test]
    fn contents_clearances_switch_on_soft() {
        assert_eq!(
            contents_clearances(false),
            (CONTENTS_CLEARANCE_PER_SIDE_MM, CONTENTS_HEIGHT_CLEARANCE_MM)
        );
        assert_eq!(
            contents_clearances(true),
            (
                CONTENTS_SOFT_CLEARANCE_PER_SIDE_MM,
                CONTENTS_SOFT_HEIGHT_CLEARANCE_MM
            )
        );
    }

    #[test]
    fn the_contents_weight_limit_is_inclusive() {
        assert!(weight_within_limit(None));
        assert!(weight_within_limit(Some(10.0)));
        assert!(!weight_within_limit(Some(10.1)));
    }

    #[test]
    fn contents_are_optional() {
        assert!(warnings(MINIMAL).is_empty());
    }

    #[test]
    fn a_4x4_12u_body_warns_about_mass_and_print_time() {
        let yaml = MINIMAL
            .replace("footprint: [1, 1]", "footprint: [4, 4]")
            .replace("height_u: 3", "height_u: 12")
            .replace("lid: false", "lid: true");
        let validation = validation(&yaml);

        assert!(validation.is_valid(), "{:?}", validation.errors);
        assert!(
            validation
                .warnings
                .iter()
                .any(|warning| warning.contains("estimated mass")),
            "{:?}",
            validation.warnings
        );
        assert!(
            validation
                .warnings
                .iter()
                .any(|warning| warning.contains("estimated print time")),
            "{:?}",
            validation.warnings
        );
    }

    #[test]
    fn a_4x4_6u_body_is_below_both_estimate_thresholds() {
        let yaml = MINIMAL
            .replace("footprint: [1, 1]", "footprint: [4, 4]")
            .replace("height_u: 3", "height_u: 6");

        assert!(
            warnings(&yaml).is_empty(),
            "the template-sized body must pass: {:?}",
            warnings(&yaml)
        );
    }

    #[test]
    fn schema_errors_are_collected_together() {
        let yaml = MINIMAL
            .replace("system: SYS-1.0", "system: SYS-9.9")
            .replace("1x1-3u-bits-v1", "Bad_ID")
            .replace("footprint: [1, 1]", "footprint: [5, 4]")
            .replace("height_u: 3", "height_u: 7");
        let errors = errors(&yaml);

        for field in ["system", "id", "footprint", "height_u"] {
            assert!(
                errors.iter().any(|error| error.contains(field)),
                "missing {field} error: {errors:?}"
            );
        }
        assert!(errors.len() >= 4, "{errors:?}");
    }

    #[test]
    fn warnings_do_not_make_a_spec_invalid() {
        let yaml = with_divisions(
            "  axis: x\n  at_mm: 14\n  near: { compartment: Thin }\n  far: { compartment: Wide }\n",
        );
        let validation = validation(&yaml);

        assert!(validation.is_valid(), "{:?}", validation.errors);
        assert_eq!(validation.warnings.len(), 1);
        assert!(validation.warnings[0].contains("15 mm"));
    }

    #[test]
    fn report_contains_the_contract_summary() {
        let spec = parse_ok(include_str!("../templates/box-spec.yaml"));
        let report = spec.report(&spec.validate());

        for expected in [
            "id: 4x4-6U-example-v1",
            "footprint: 4 x 4 (outer 167.5 x 167.5 mm)",
            "height: 6U (42 mm total)",
            "interior: 163.5 x 163.5 x 33.25 mm",
            "lid: yes",
            "compartments: 4",
            "USB-C short",
            "Power",
            "estimated mass:",
            "estimated print time:",
            "warnings: none",
        ] {
            assert!(report.contains(expected), "missing {expected:?}:\n{report}");
        }
    }

    #[test]
    fn report_lists_unnamed_leaves() {
        let yaml = with_divisions("  {}\n");
        let spec = parse_ok(&yaml);
        let report = spec.report(&spec.validate());

        assert!(report.contains("compartments: 1"), "{report}");
        assert!(report.contains("(unnamed)"), "{report}");
    }

    #[test]
    fn report_lists_warnings() {
        let yaml = with_divisions(
            "  axis: x\n  at_mm: 14\n  near: { compartment: Thin }\n  far: { compartment: Wide }\n",
        );
        let spec = parse_ok(&yaml);
        let report = spec.report(&spec.validate());

        assert!(report.contains("warnings:"), "{report}");
        assert!(report.contains("15 mm"), "{report}");
    }

    #[test]
    fn body_geometry_is_derived_from_footprint_and_height() {
        let spec = parse_ok(include_str!("../templates/box-spec.yaml"));
        let body = spec.body();

        assert_eq!(body, BodyGeometry::new([4, 4], 6));
    }

    /// The canonical writer round-trips every fixture and the template: the
    /// rendered text parses to an equal spec and is stable under re-rendering.
    #[test]
    fn canonical_yaml_round_trips_every_fixture() {
        for text in [
            MINIMAL,
            include_str!("../tests/fixtures/4x4-6u.yaml"),
            include_str!("../tests/fixtures/4x4-6u-tree.yaml"),
            include_str!("../tests/fixtures/2x4-6u-lid.yaml"),
            include_str!("../tests/fixtures/2x4-6u-lid-ribs.yaml"),
            include_str!("../templates/box-spec.yaml"),
        ] {
            let spec = parse_ok(text);
            let rendered = spec.to_yaml();
            let reparsed = parse(&rendered)
                .unwrap_or_else(|error| panic!("canonical YAML must parse ({error}):\n{rendered}"));
            assert_eq!(
                reparsed, spec,
                "canonical YAML changed the spec:\n{rendered}"
            );
            assert_eq!(
                reparsed.to_yaml(),
                rendered,
                "canonical YAML must be stable"
            );
        }
    }

    #[test]
    fn canonical_yaml_is_the_documented_layout() {
        let spec = parse_ok(MINIMAL);
        assert_eq!(
            spec.to_yaml(),
            "system: SYS-1.0\n\
             id: 1x1-3u-bits-v1\n\
             footprint: [1, 1]\n\
             height_u: 3\n\
             lid: false\n\
             divisions: {compartment: \"Bits\"}\n"
        );

        let unnamed =
            parse_ok(&MINIMAL.replace("divisions: { compartment: Bits }", "divisions: {}"));
        assert_eq!(
            unnamed.to_yaml(),
            "system: SYS-1.0\n\
             id: 1x1-3u-bits-v1\n\
             footprint: [1, 1]\n\
             height_u: 3\n\
             lid: false\n\
             divisions: {}\n"
        );
    }

    #[test]
    fn canonical_yaml_quotes_free_text_and_numbers() {
        let text = concat!(
            "system: SYS-1.0\n",
            "id: 2x4-6U-thing-v1\n",
            "footprint: [2, 4]\n",
            "height_u: 6\n",
            "lid: true\n",
            "lid_ribs: true\n",
            "notes: \"A \\\"quoted\\\" note\"\n",
            "contents:\n",
            "  description: Cables\n",
            "  soft: true\n",
            "  max_width_mm: 79.5\n",
            "  weight_kg: 2\n",
            "divisions:\n",
            "  axis: x\n",
            "  at_mm: 39\n",
            "  near: { compartment: \"A\" }\n",
            "  far: {}\n",
        );
        let expected = concat!(
            "system: SYS-1.0\n",
            "id: 2x4-6U-thing-v1\n",
            "footprint: [2, 4]\n",
            "height_u: 6\n",
            "lid: true\n",
            "lid_ribs: true\n",
            "notes: \"A \\\"quoted\\\" note\"\n",
            "contents:\n",
            "  description: \"Cables\"\n",
            "  soft: true\n",
            "  max_width_mm: 79.5\n",
            "  weight_kg: 2.0\n",
            "divisions:\n",
            "  axis: x\n",
            "  at_mm: 39\n",
            "  near: {compartment: \"A\"}\n",
            "  far: {}\n",
        );

        assert_eq!(parse_ok(text).to_yaml(), expected);
    }

    /// The canonical writer never emits the internal coupon flag or any fixed
    /// architecture field: the schema stays closed.
    #[test]
    fn canonical_yaml_never_emits_internal_fields() {
        let spec = parse_ok(MINIMAL);
        let rendered = spec.to_yaml();

        assert!(!rendered.contains("coupon"), "{rendered}");
        for field in ["wall", "radius", "fillet", "clearance", "cove"] {
            assert!(!rendered.contains(field), "{rendered}");
        }
    }
}
