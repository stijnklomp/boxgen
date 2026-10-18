//! Build-directory files: `GET /api/artifacts/{id}` (the listing),
//! `GET /api/artifacts/{id}/{name}` (the bytes the 3D preview loads), and
//! `POST /api/export`.
//!
//! All three endpoints read the directory the one build pipeline wrote
//! (`build/<id>`) and never invent paths: the listing endpoint lists what is
//! there, the bytes endpoint serves exactly the generated artifact names with
//! their content types, and export copies the named print files into
//! `exports/<base>/` through [`crate::build::export_named_copies`], so the
//! naming and overwrite rules live in one place.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};

use crate::build;
use crate::spec;

use super::dto::{ArtifactDto, ArtifactsResponse, ExportRequest, ExportResponse};
use super::{ApiError, Workspace, parse_body};

/// `GET /api/artifacts/{id}` — the files in one build directory.
///
/// The listing is the build directory as it is on disk, so it also works
/// after a server restart; `cad/` and other directories are skipped.
pub(crate) async fn artifacts(
    State(workspace): State<Workspace>,
    UrlPath(id): UrlPath<String>,
) -> Result<Json<ArtifactsResponse>, ApiError> {
    if !spec::is_valid_id(&id) {
        return Err(ApiError::not_found(format!("no build {id:?}")));
    }

    let out_dir = workspace.build_dir(&id);
    let mut artifacts = Vec::new();
    for path in build_files(&out_dir)? {
        let bytes = fs::metadata(&path)
            .map_err(|error| {
                ApiError::internal(format!("cannot inspect `{}`: {error}", path.display()))
            })?
            .len();
        let artifact = build::Artifact {
            name: file_name(&path),
            path,
            bytes,
        };
        artifacts.push(ArtifactDto::from_artifact(&workspace, &artifact));
    }

    Ok(Json(ArtifactsResponse {
        id,
        out_dir: workspace.display(&out_dir),
        artifacts,
    }))
}

/// `GET /api/artifacts/{id}/{name}` — the bytes of one known build file.
///
/// This is what the 3D preview loads. The name must be one of the generated
/// artifacts ([`crate::build::GENERATED_ARTIFACTS`]) and the file must exist;
/// anything else is a 404, so the route can never serve `spec.yaml`, the
/// master, or a file outside `build/<id>`.
pub(crate) async fn artifact_bytes(
    State(workspace): State<Workspace>,
    UrlPath((id, name)): UrlPath<(String, String)>,
) -> Result<Response, ApiError> {
    if !spec::is_valid_id(&id) {
        return Err(ApiError::not_found(format!("no build {id:?}")));
    }
    if !build::GENERATED_ARTIFACTS.contains(&name.as_str()) {
        return Err(ApiError::not_found(format!(
            "no artifact {name:?} in the build of `{id}`"
        )));
    }

    let path = workspace.build_dir(&id).join(&name);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(ApiError::not_found(format!(
                "no artifact `{name}` in `{}`; run a build first",
                workspace.build_dir(&id).display()
            )));
        }
        Err(error) => {
            return Err(ApiError::internal(format!(
                "cannot read `{}`: {error}",
                path.display()
            )));
        }
    };

    Ok((
        [
            (header::CONTENT_TYPE, artifact_content_type(&name)),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response())
}

/// The content type of one generated artifact, by extension.
fn artifact_content_type(name: &str) -> &'static str {
    match Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some("stl") => "model/stl",
        Some("step") => "model/step",
        Some("json") => "application/json",
        Some("py") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// `POST /api/export` — copies one build's print files under `base` names.
pub(crate) async fn export(
    State(workspace): State<Workspace>,
    body: Bytes,
) -> Result<Json<ExportResponse>, ApiError> {
    let ExportRequest { base } = parse_body(&body)?;
    if !spec::is_valid_id(&base) {
        return Err(ApiError::invalid(
            format!("export base {base:?} is not a valid box id"),
            vec![format!(
                "base: {base:?} must be a box id so the build at build/<base> is found and exports/<base> stays inside the workspace"
            )],
        ));
    }

    let artifact_dir = workspace.build_dir(&base);
    if !artifact_dir.is_dir() {
        return Err(ApiError::not_found(format!(
            "no build for `{base}` in `{}`; run a build first",
            artifact_dir.display()
        )));
    }

    let out_dir = workspace.exports_dir().join(&base);
    let artifacts = build::export_named_copies(&base, &artifact_dir, &out_dir)
        .map_err(ApiError::from_boxgen)?;

    Ok(Json(ExportResponse {
        base,
        out_dir: workspace.display(&out_dir),
        artifacts: artifacts
            .iter()
            .map(|artifact| ArtifactDto::from_artifact(&workspace, artifact))
            .collect(),
    }))
}

/// Lists the regular files directly in a build directory, sorted by name.
fn build_files(out_dir: &Path) -> Result<Vec<PathBuf>, ApiError> {
    let entries = match fs::read_dir(out_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ApiError::not_found(format!(
                "no build in `{}`; run a build first",
                out_dir.display()
            )));
        }
        Err(error) => {
            return Err(ApiError::internal(format!(
                "cannot read `{}`: {error}",
                out_dir.display()
            )));
        }
    };

    let mut files = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            ApiError::internal(format!("cannot read `{}`: {error}", out_dir.display()))
        })?
        .into_iter()
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_file()))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

/// The file name of a path, for display.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
