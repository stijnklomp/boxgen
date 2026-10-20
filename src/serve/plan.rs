//! `POST /api/plan`: the resolved layout for one design.
//!
//! The plan is pure Rust: [`crate::spec::Spec::validate`] runs the same rules
//! the CLI runs and [`crate::tree::validate_tree`] is the same walker, so the
//! preview cannot disagree with the generator (ADR-0002: no geometry in JS).
//! Every edit is planned before it is saved; an invalid design still returns
//! its errors and, when the footprint and height are in range, the layout it
//! would produce.

use axum::Json;
use axum::body::Bytes;

use super::ApiError;
use super::dto::{
    BodyDto, Design, DesignRequest, DividerRectDto, EstimateDto, LeafRectDto, PlanResponse,
};
use super::parse_body;

/// `POST /api/plan`.
pub(crate) async fn plan(body: Bytes) -> Result<Json<PlanResponse>, ApiError> {
    let DesignRequest { design } = parse_body(&body)?;

    Ok(Json(plan_design(&design)))
}

/// Resolves one design into the wire plan.
pub(crate) fn plan_design(design: &Design) -> PlanResponse {
    let spec = design.to_spec();
    let validation = spec.validate();
    let body = spec.body();

    PlanResponse {
        valid: validation.is_valid(),
        errors: validation.errors,
        warnings: validation.warnings,
        body: BodyDto::from_geometry(&body),
        leaves: validation
            .leaves
            .iter()
            .map(|leaf| LeafRectDto::from_leaf(leaf, &body))
            .collect(),
        dividers: validation
            .dividers
            .iter()
            .map(DividerRectDto::from)
            .collect(),
        estimate: EstimateDto::from(validation.estimate),
    }
}
