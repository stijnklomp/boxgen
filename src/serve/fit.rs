//! `POST /api/fit`: viable footprint and height combinations for one item.
//!
//! The check is the standard §8 contents fit — the same
//! [`crate::spec::Contents::fit`] predicate the validator warns from — so a
//! fit answer and a `contents` warning on the same numbers can never
//! disagree. The candidates are every legal footprint and height; the search
//! deliberately does not try rotated orientations, because the spec's check
//! does not either.

use axum::Json;
use axum::body::Bytes;

use crate::spec::{self, Contents};
use crate::tree::{self, BodyGeometry};

use super::ApiError;
use super::dto::{FitCandidateDto, FitClearancesDto, FitRequest, FitResponse};
use super::parse_body;

/// `POST /api/fit`.
pub(crate) async fn fit(body: Bytes) -> Result<Json<FitResponse>, ApiError> {
    let request: FitRequest = parse_body(&body)?;
    check_item(&request)?;

    Ok(Json(fit_item(&request)))
}

/// Rejects an item that cannot be measured.
fn check_item(request: &FitRequest) -> Result<(), ApiError> {
    for (name, value) in [
        ("length_mm", request.length_mm),
        ("width_mm", request.width_mm),
        ("height_mm", request.height_mm),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(ApiError::bad_request(format!(
                "{name} must be a positive number of millimetres"
            )));
        }
    }
    if let Some(weight) = request.weight_kg
        && (!weight.is_finite() || weight < 0.0)
    {
        return Err(ApiError::bad_request(
            "weight_kg must be zero or a positive number of kilograms",
        ));
    }

    Ok(())
}

/// Resolves the viable combinations for one item.
pub(crate) fn fit_item(request: &FitRequest) -> FitResponse {
    let contents = Contents {
        description: None,
        soft: request.soft,
        max_length_mm: Some(request.length_mm),
        max_width_mm: Some(request.width_mm),
        max_height_mm: Some(request.height_mm),
        weight_kg: request.weight_kg,
    };

    let mut viable = Vec::new();
    for n in tree::MIN_FOOTPRINT_UNITS..=tree::MAX_FOOTPRINT_UNITS {
        for m in tree::MIN_FOOTPRINT_UNITS..=tree::MAX_FOOTPRINT_UNITS {
            for &height_u in &tree::HEIGHTS_U {
                let body = BodyGeometry::new([n, m], height_u);
                if contents.fit(&body).fits() {
                    viable.push(FitCandidateDto {
                        footprint: [n, m],
                        height_u,
                    });
                }
            }
        }
    }
    viable.sort_by_key(|candidate| {
        (
            candidate.footprint[0] * candidate.footprint[1],
            candidate.footprint[0],
            candidate.footprint[1],
            candidate.height_u,
        )
    });

    let (per_side_mm, height_clearance_mm) = spec::contents_clearances(request.soft);

    FitResponse {
        soft: request.soft,
        weight_ok: spec::weight_within_limit(request.weight_kg),
        clearances: FitClearancesDto {
            per_side_mm,
            height_clearance_mm,
            diagonal_margin_mm: spec::CONTENTS_DIAGONAL_MARGIN_MM,
        },
        viable,
    }
}
