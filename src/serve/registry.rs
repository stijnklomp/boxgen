//! Registry-side actions: `POST /api/publish` and `POST /api/index`.
//!
//! Publish copies the current `build/<id>` into `boxes/<id>/` — the one
//! `--publish` target rule, shared through
//! [`crate::build::box_folder_named_after`]. The registry tolerates a legacy
//! folder name for `open` and `save` (like `boxgen index` does), but publish
//! never does: the target is always the folder named after the id. Every
//! `--publish` rule holds: all required artifacts are verified before the
//! first copy, `notes.md` is never overwritten, and `box.FCStd`,
//! `report.json`, `box.py`, and `cad/` are never copied.
//!
//! Index regenerates `boxes/INDEX.md` through the CLI's one renderer
//! ([`crate::index::regenerate`]), so the bytes match `boxgen index` and the
//! strict all-or-nothing scan still applies.

use std::fs;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;

use crate::build;
use crate::index;
use crate::spec;

use super::dto::{IndexResponse, PublishRequest, PublishResponse};
use super::store::{SPEC_FILE, find_box};
use super::{ApiError, Workspace, parse_body};

/// `POST /api/publish` — copies the current build into the box folder.
pub(crate) async fn publish(
    State(workspace): State<Workspace>,
    body: Bytes,
) -> Result<Json<PublishResponse>, ApiError> {
    let PublishRequest { id } = parse_body(&body)?;

    let folder = find_box(&workspace.boxes_dir(), &id)
        .ok_or_else(|| ApiError::not_found(format!("no box {id:?} in the registry")))?;
    let spec_path = folder.join(SPEC_FILE);

    // The one publish rule, shared with `--publish`: the target is
    // `boxes/<id>`, so a legacy folder name is a request error, not a second
    // publish location.
    let folder = build::box_folder_named_after(&spec_path, &id).ok_or_else(|| {
        ApiError::bad_request(format!(
            "cannot publish `{}`: the box folder must be named after its id `{id}`; \
             rename it to `boxes/{id}/` or use Save as to create that folder",
            workspace.display(&spec_path)
        ))
    })?;

    let spec = spec::load(&spec_path).map_err(ApiError::from_boxgen)?;
    if spec.id != id {
        return Err(ApiError::conflict(format!(
            "the spec in `{}` has id {:?}, not {id:?}; \
             publish targets `boxes/<id>/` and never writes a box under a mismatched id",
            workspace.display(&spec_path),
            spec.id
        )));
    }
    let validation = spec.validate();
    if !validation.is_valid() {
        return Err(ApiError::invalid(
            "the saved box spec is not valid",
            validation.errors,
        ));
    }

    let out_dir = workspace.build_dir(&id);
    if !out_dir.is_dir() {
        return Err(ApiError::not_found(format!(
            "no build for `{id}` in `{}`; run a build first",
            out_dir.display()
        )));
    }

    build::publish_artifacts(&spec, &out_dir, &folder).map_err(ApiError::from_boxgen)?;

    Ok(Json(PublishResponse {
        id,
        published_dir: workspace.display(&folder),
    }))
}

/// `POST /api/index` — regenerates `boxes/INDEX.md`.
///
/// A fresh workspace may not have a registry directory yet (`git` cannot track
/// an empty one); the endpoint creates it and writes the committed empty
/// index, exactly as the list endpoint treats a missing registry as empty.
/// The scan itself stays strict: one invalid spec fails the whole run.
pub(crate) async fn index(
    State(workspace): State<Workspace>,
) -> Result<Json<IndexResponse>, ApiError> {
    let boxes = workspace.boxes_dir();
    if !boxes.is_dir() {
        fs::create_dir_all(&boxes).map_err(|error| {
            ApiError::internal(format!("cannot create `{}`: {error}", boxes.display()))
        })?;
    }

    let document = index::regenerate(&boxes).map_err(ApiError::from_boxgen)?;

    Ok(Json(IndexResponse {
        path: workspace.display(&boxes.join("INDEX.md")),
        boxes: document.boxes,
    }))
}
