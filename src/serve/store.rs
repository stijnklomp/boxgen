//! `boxes/` registry endpoints: list, open, save.
//!
//! The store reads and writes the same `boxes/<id>/spec.yaml` layout the CLI
//! uses (`src/index.rs`, `--publish`). A save is canonical: the design is
//! rendered by [`crate::spec::Spec::to_yaml`], re-parsed through
//! [`crate::spec::parse`], validated, and only then written through
//! `spec.yaml.tmp` plus a rename — a rejected or interrupted save never
//! leaves a partial spec, and no invalid file reaches disk.
//!
//! Folder resolution follows `boxgen index`: the standard layout is
//! `boxes/<id>/`, but a folder name may differ from the spec id, so a
//! direct-path miss falls back to scanning the registry for a matching id.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};

use crate::spec;

use super::dto::{BoxListResponse, BoxSummary, Design, DesignRequest, OpenResponse, SaveResponse};
use super::{ApiError, Workspace, parse_body};

/// The spec file name inside every registry folder.
pub(crate) const SPEC_FILE: &str = "spec.yaml";

/// Temporary file written before the atomic rename.
const SPEC_TEMP_FILE: &str = "spec.yaml.tmp";

/// `GET /api/boxes` — every registry folder that has a spec.
pub(crate) async fn list(
    State(workspace): State<Workspace>,
) -> Result<Json<BoxListResponse>, ApiError> {
    let boxes = workspace.boxes_dir();
    let folders = match list_folders(&boxes) {
        Ok(folders) => folders,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return Err(ApiError::internal(format!(
                "cannot read the box registry `{}`: {error}",
                boxes.display()
            )));
        }
    };

    let mut summaries = folders
        .iter()
        .map(|folder| summary(&workspace, folder))
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.folder.cmp(&right.folder))
    });

    Ok(Json(BoxListResponse { boxes: summaries }))
}

/// `POST /api/boxes/{id}/open` — the raw spec text plus the parsed design.
pub(crate) async fn open(
    State(workspace): State<Workspace>,
    UrlPath(id): UrlPath<String>,
) -> Result<Json<OpenResponse>, ApiError> {
    let folder = find_box(&workspace.boxes_dir(), &id)
        .ok_or_else(|| ApiError::not_found(format!("no box {id:?} in the registry")))?;
    let spec_path = folder.join(SPEC_FILE);
    let text = fs::read_to_string(&spec_path).map_err(|error| {
        ApiError::internal(format!("cannot read `{}`: {error}", spec_path.display()))
    })?;
    let display_path = workspace.display(&spec_path);

    match spec::parse(&text) {
        Ok(spec) => {
            let validation = spec.validate();
            Ok(Json(OpenResponse {
                id: spec.id.clone(),
                path: display_path,
                text,
                design: Some(Design::from_spec(&spec)),
                valid: validation.is_valid(),
                errors: validation.errors,
                warnings: validation.warnings,
            }))
        }
        Err(error) => Ok(Json(OpenResponse {
            id,
            path: display_path,
            text,
            design: None,
            valid: false,
            errors: vec![error.to_string()],
            warnings: Vec::new(),
        })),
    }
}

/// `POST /api/boxes/{id}/save` — validate and write the canonical spec.
///
/// The `:id` path segment is the registry key: it must be a valid box id and
/// must equal the design's id (renaming is a non-goal). A missing folder is
/// created.
pub(crate) async fn save(
    State(workspace): State<Workspace>,
    UrlPath(id): UrlPath<String>,
    body: Bytes,
) -> Result<Json<SaveResponse>, ApiError> {
    let DesignRequest { design } = parse_body(&body)?;

    if !spec::is_valid_id(&id) {
        return Err(ApiError::invalid(
            format!("registry key {id:?} is not a valid box id"),
            vec![format!(
                "id: {id:?} must be lowercase kebab-case or the standard <N>x<M>-<height>U-<slug>-<rev> form (for example 4x4-6U-usb-cables-v1)"
            )],
        ));
    }
    if design.id != id {
        return Err(ApiError::invalid(
            format!(
                "design id {:?} does not match the registry key {id:?}",
                design.id
            ),
            vec![format!(
                "id: the design id must equal the registry key; save to /api/boxes/{}/save instead",
                design.id
            )],
        ));
    }

    let folder = find_box(&workspace.boxes_dir(), &id).unwrap_or_else(|| {
        // A new box: the standard layout is the registry key.
        workspace.boxes_dir().join(&id)
    });
    let spec_path = folder.join(SPEC_FILE);

    // One canonical writer, then the real parser plus the contract rules as a
    // backstop: a design can never reach disk as a spec the CLI would reject.
    let spec = design.to_spec();
    let text = spec.to_yaml();
    let parsed = spec::parse(&text).map_err(|error| {
        ApiError::invalid(
            "the canonical rendering does not parse as a spec",
            vec![error.to_string()],
        )
    })?;
    if parsed != spec {
        return Err(ApiError::internal(
            "the canonical rendering did not round-trip to the same spec",
        ));
    }
    let validation = parsed.validate();
    if !validation.is_valid() {
        return Err(ApiError::invalid(
            "the design is not valid",
            validation.errors,
        ));
    }

    write_spec(&spec_path, &text).map_err(|error| {
        ApiError::internal(format!("cannot save `{}`: {error}", spec_path.display()))
    })?;

    Ok(Json(SaveResponse {
        id,
        path: workspace.display(&spec_path),
        text,
        warnings: validation.warnings,
    }))
}

/// Lists the registry folders with a `spec.yaml`, sorted by name.
fn list_folders(boxes: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut folders = fs::read_dir(boxes)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_dir()))
        .map(|entry| entry.path())
        .filter(|path| path.join(SPEC_FILE).is_file())
        .collect::<Vec<_>>();
    folders.sort();
    Ok(folders)
}

/// Resolves the registry folder for a box id.
///
/// The direct-path shortcut only runs for a valid box id — no separators, so
/// it can never leave `boxes/`. Because `boxgen index` allows folder names
/// that differ from the spec id, a miss falls back to scanning the registry
/// for a spec whose id matches (a pure string comparison). A folder whose name
/// is not a valid box id is matched by folder name too, so a broken spec
/// (whose id cannot be read) is still openable.
pub(crate) fn find_box(boxes: &Path, id: &str) -> Option<PathBuf> {
    if spec::is_valid_id(id) {
        let direct = boxes.join(id);
        if direct.join(SPEC_FILE).is_file() {
            return Some(direct);
        }
    }

    list_folders(boxes).ok()?.into_iter().find(|folder| {
        spec_id(&folder.join(SPEC_FILE)).as_deref() == Some(id)
            || folder.file_name().and_then(|name| name.to_str()) == Some(id)
    })
}

/// Reads the spec id from a spec file, if it parses.
fn spec_id(spec_path: &Path) -> Option<String> {
    let text = fs::read_to_string(spec_path).ok()?;
    spec::parse(&text).ok().map(|spec| spec.id)
}

/// Builds one list row.
fn summary(workspace: &Workspace, folder: &Path) -> BoxSummary {
    let spec_path = folder.join(SPEC_FILE);
    let folder_name = folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let path = workspace.display(&spec_path);
    let mtime = mtime_seconds(&spec_path);

    let Ok(text) = fs::read_to_string(&spec_path) else {
        return broken(
            folder_name,
            path,
            "cannot read the spec file".to_owned(),
            mtime,
        );
    };

    match spec::parse(&text) {
        Ok(spec) => {
            let validation = spec.validate();
            BoxSummary {
                id: spec.id.clone(),
                folder: folder_name,
                footprint: Some(spec.footprint),
                height_u: Some(spec.height_u),
                lid: Some(spec.lid),
                lid_ribs: Some(spec.lid_ribs),
                valid: validation.is_valid(),
                errors: validation.errors,
                path,
                mtime,
            }
        }
        Err(error) => broken(folder_name, path, error.to_string(), mtime),
    }
}

/// A row for a spec that cannot be read or parsed.
fn broken(folder: String, path: String, error: String, mtime: u64) -> BoxSummary {
    BoxSummary {
        id: folder.clone(),
        folder,
        footprint: None,
        height_u: None,
        lid: None,
        lid_ribs: None,
        valid: false,
        errors: vec![error],
        path,
        mtime,
    }
}

/// Seconds since the Unix epoch, or 0 when unavailable.
fn mtime_seconds(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs())
}

/// Writes `spec.yaml` through `spec.yaml.tmp` plus a rename.
fn write_spec(spec_path: &Path, text: &str) -> Result<(), String> {
    let folder = spec_path.parent().expect("a spec path always has a parent");
    fs::create_dir_all(folder)
        .map_err(|error| format!("cannot create `{}`: {error}", folder.display()))?;

    let temp = spec_path.with_file_name(SPEC_TEMP_FILE);
    if let Err(error) = fs::write(&temp, text) {
        let _ = fs::remove_file(&temp);
        return Err(format!("cannot write `{}`: {error}", temp.display()));
    }
    fs::rename(&temp, spec_path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!(
            "cannot replace `{}` atomically: {error}",
            spec_path.display()
        )
    })
}
