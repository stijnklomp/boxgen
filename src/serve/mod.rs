//! `boxgen serve`: the local web UI over the spec (v2, ADR-0002).
//!
//! One binary, one local user: the server binds `127.0.0.1` only, serves the
//! embedded frontend, and exposes the API (plan, specs, fit, standard, and
//! the one build job with its export, publish, and index actions). Nothing
//! here owns geometry or rules: every request is answered by [`crate::spec`],
//! [`crate::tree`], [`crate::build`], and [`crate::index`] — the same code the
//! CLI runs, so the UI can never drift from the generator.
//!
//! The wire schema is documented in `src/serve/dto.rs`. stdout carries one
//! report line (the URL); tracing stays on stderr, and a build never prints
//! the CLI summary.

mod assets;
mod dto;
mod files;
mod fit;
mod jobs;
mod plan;
mod registry;
mod standard;
mod store;

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::json;

use crate::config::Config;
use crate::error::BoxgenError;

/// Everything a request handler needs to know about the workspace.
///
/// The server resolves `root` from the process working directory (the mounted
/// repository) and shows paths under `config.ui_host_workdir`, which is the
/// same directory as the user sees it on their machine
/// (`BOXGEN_UI_HOST_WORKDIR` inside the container, ticket #20). The one build
/// job and the FreeCAD launcher live here too, so a request never reaches for
/// process state.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// Workspace root: `boxes/`, `build/`, and `exports/` resolve under it.
    pub root: PathBuf,
    config: Config,
    jobs: jobs::Jobs,
}

impl Workspace {
    /// Builds a workspace over `root` with the given runtime configuration.
    ///
    /// Displayed paths use `config.ui_host_workdir` when set, else `root`.
    pub fn new(root: impl Into<PathBuf>, config: Config) -> Self {
        Self {
            root: root.into(),
            config,
            jobs: jobs::Jobs::default(),
        }
    }

    /// Resolves the workspace from the process working directory and config.
    pub fn from_config(config: &Config) -> Result<Self, BoxgenError> {
        let root = std::env::current_dir().map_err(|error| BoxgenError::Environment {
            message: format!("cannot resolve the working directory for the local UI: {error}"),
        })?;

        Ok(Self::new(root, config.clone()))
    }

    /// The runtime configuration, including the FreeCAD launcher.
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    /// The one build job of this server process.
    pub(crate) fn jobs(&self) -> &jobs::Jobs {
        &self.jobs
    }

    /// Registry directory for the store endpoints.
    pub(crate) fn boxes_dir(&self) -> PathBuf {
        self.root.join("boxes")
    }

    /// Build directory for one box id.
    pub(crate) fn build_dir(&self, id: &str) -> PathBuf {
        self.root.join("build").join(id)
    }

    /// Export root for the named print copies.
    pub(crate) fn exports_dir(&self) -> PathBuf {
        self.root.join("exports")
    }

    /// The host path the user sees for a workspace path.
    pub(crate) fn display(&self, path: &Path) -> String {
        let host_workdir = self
            .config
            .ui_host_workdir
            .as_deref()
            .unwrap_or(self.root.as_path());
        match path.strip_prefix(&self.root) {
            Ok(relative) => host_workdir.join(relative).display().to_string(),
            Err(_) => path.display().to_string(),
        }
    }
}

/// Builds the UI router over one workspace.
///
/// This is the seam the API tests drive in process
/// (`tower::ServiceExt::oneshot`); no port is bound here.
pub fn app(workspace: Workspace) -> Router {
    Router::new()
        .route("/", get(assets::index))
        .route("/assets/{*path}", get(assets::asset))
        .route("/api/health", get(health))
        .route("/api/standard", get(standard::get))
        .route("/api/boxes", get(store::list))
        .route("/api/boxes/{id}/open", post(store::open))
        .route("/api/boxes/{id}/save", post(store::save))
        .route("/api/plan", post(plan::plan))
        .route("/api/fit", post(fit::fit))
        .route("/api/build", post(jobs::start))
        .route("/api/build/status", get(jobs::status))
        .route("/api/artifacts/{id}", get(files::artifacts))
        .route("/api/artifacts/{id}/{name}", get(files::artifact_bytes))
        .route("/api/export", post(files::export))
        .route("/api/publish", post(registry::publish))
        .route("/api/index", post(registry::index))
        .with_state(workspace)
}

/// `GET /api/health` — what the packaging script and the frontend probe.
async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "system_version": crate::spec::SYSTEM_VERSION,
    }))
}

/// The one JSON error shape every endpoint returns.
pub(crate) struct ApiError {
    status: StatusCode,
    message: String,
    errors: Vec<String>,
}

impl ApiError {
    /// 400: the request body is not the documented wire schema.
    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            errors: Vec::new(),
        }
    }

    /// 404: the box or asset does not exist.
    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
            errors: Vec::new(),
        }
    }

    /// 422: the design or spec violates the contract rules.
    ///
    /// `errors` carries the schema-path messages (`divisions.near: …`), which
    /// the UI shows verbatim.
    pub(crate) fn invalid(message: impl Into<String>, errors: Vec<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: message.into(),
            errors,
        }
    }

    /// 409: the request conflicts with the state of the workspace, for
    /// example a second build while one runs or an incomplete build being
    /// exported or published.
    pub(crate) fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
            errors: Vec::new(),
        }
    }

    /// 500: a local file operation failed; a server or workspace problem.
    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
            errors: Vec::new(),
        }
    }

    /// Maps a synchronous pipeline failure onto the wire shape.
    ///
    /// Exit codes stay the CLI's contract; the HTTP mapping is: a build-state
    /// conflict (incomplete artifacts, a failed copy step) is 409, an invalid
    /// spec is 422, a usage problem is 400, and everything local is 500. A
    /// job does not use this: its failure travels in the job status payload.
    pub(crate) fn from_boxgen(error: BoxgenError) -> Self {
        match error {
            BoxgenError::Build { .. } => Self::conflict(error.to_string()),
            BoxgenError::InvalidSpec { .. } => Self::invalid(error.to_string(), Vec::new()),
            BoxgenError::Usage { .. } => Self::bad_request(error.to_string()),
            BoxgenError::NotImplemented { .. }
            | BoxgenError::Io { .. }
            | BoxgenError::Environment { .. } => Self::internal(error.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": self.message,
                "errors": self.errors,
            })),
        )
            .into_response()
    }
}

/// Parses a JSON request body, mapping every failure to a 400.
pub(crate) fn parse_body<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(body)
        .map_err(|error| ApiError::bad_request(format!("invalid request body: {error}")))
}

/// Runs the local UI until the process is stopped.
///
/// The URL report line goes to stdout (stderr carries diagnostics) and the
/// server binds `127.0.0.1` only. With `open`, `xdg-open` is spawned and any
/// failure is ignored: the URL is already on stdout.
pub fn serve(port: Option<u16>, open: bool, config: &Config) -> Result<(), BoxgenError> {
    let port = port.unwrap_or(config.ui_port);
    let workspace = Workspace::from_config(config)?;
    let url = format!("http://127.0.0.1:{port}/");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| BoxgenError::Environment {
            message: format!("cannot start the async runtime for the local UI: {error}"),
        })?;

    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|error| BoxgenError::Environment {
                message: format!("cannot bind 127.0.0.1:{port}: {error}"),
            })?;

        println!("boxgen ui: {url}");
        if open {
            open_browser(&url);
        }

        axum::serve(listener, app(workspace))
            .await
            .map_err(|error| BoxgenError::Environment {
                message: format!("the local UI stopped with an error: {error}"),
            })
    })
}

/// Spawns `xdg-open` for the URL; a missing browser never fails the server.
fn open_browser(url: &str) {
    match Command::new("xdg-open")
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => tracing::debug!(url, "opened the local UI in the browser"),
        Err(error) => tracing::warn!(url, %error, "could not run xdg-open"),
    }
}
