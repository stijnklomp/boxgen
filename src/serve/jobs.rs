//! The one build job behind `POST /api/build`.
//!
//! One local user means one build at a time: the job runs the CLI's
//! [`crate::build::run_build`] pipeline on a blocking thread
//! (`tokio::task::spawn_blocking` — `freecadcmd` is a blocking process), the
//! POST returns immediately, and `GET /api/build/status` reports the state:
//! idle, running (stage and elapsed time), done (the structured outcome), or
//! failed (the error plus the `freecadcmd` diagnostic tail). There is no
//! queue and no cancel: a second POST while one runs is a 409.
//!
//! The state is a `std::sync::Mutex` held only for short sections that never
//! await: the pipeline thread records progress and the async wrapper records
//! the terminal state, so a panic or a cancelled task can never leave the job
//! stuck in `running`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;

use crate::build::{self, BuildEvent, BuildOutcome, BuildStage};
use crate::config::Config;
use crate::spec::Spec;

use super::dto::{BuildOutcomeDto, BuildStatusResponse, DesignRequest};
use super::{ApiError, Workspace, parse_body};

/// The single build job of one server process.
#[derive(Debug, Clone)]
pub(crate) struct Jobs {
    state: Arc<Mutex<Job>>,
}

impl Default for Jobs {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(Job::Idle)),
        }
    }
}

/// The job's state.
#[derive(Debug, Clone)]
enum Job {
    /// No build has run in this server process.
    Idle,
    /// One build is running.
    Running {
        /// Box id being built.
        id: String,
        /// Current phase.
        stage: BuildStage,
        /// When the job started, for the elapsed time.
        started: Instant,
        /// `freecadcmd` output tail, recorded when the process failed.
        diagnostic: Option<String>,
    },
    /// The last build finished and every invariant held.
    Done(Box<BuildOutcome>),
    /// The last build failed.
    Failed {
        /// The exit-contract message.
        error: String,
        /// `freecadcmd` output tail, when the process produced one.
        diagnostic: Option<String>,
    },
}

/// A snapshot of the job for the status endpoint.
#[derive(Debug, Clone)]
pub(crate) enum JobSnapshot {
    /// No build has run in this server process.
    Idle,
    /// One build is running.
    Running {
        /// Box id being built.
        id: String,
        /// Current phase.
        stage: BuildStage,
        /// Milliseconds since the build started.
        elapsed_ms: u64,
    },
    /// The last build finished.
    Done(Box<BuildOutcome>),
    /// The last build failed.
    Failed {
        /// The exit-contract message.
        error: String,
        /// `freecadcmd` output tail, when there was one.
        diagnostic: Option<String>,
    },
}

/// The job that is already running, for the busy rejection.
pub(crate) struct RunningJob {
    /// Box id of the running build.
    pub id: String,
    /// Phase the running build is in.
    pub stage: BuildStage,
}

impl Jobs {
    /// Takes a snapshot of the current state.
    pub(crate) fn snapshot(&self) -> JobSnapshot {
        let state = self.lock();
        match &*state {
            Job::Idle => JobSnapshot::Idle,
            Job::Running {
                id, stage, started, ..
            } => JobSnapshot::Running {
                id: id.clone(),
                stage: *stage,
                elapsed_ms: elapsed_ms(started),
            },
            Job::Done(outcome) => JobSnapshot::Done(outcome.clone()),
            Job::Failed { error, diagnostic } => JobSnapshot::Failed {
                error: error.clone(),
                diagnostic: diagnostic.clone(),
            },
        }
    }

    /// Starts one build unless another is running.
    ///
    /// The check-and-set happens under the lock, so two simultaneous POSTs
    /// cannot both start. The build runs on a blocking thread; a panic or a
    /// cancelled task is recorded as a failure instead of leaving the job
    /// stuck.
    pub(crate) fn try_start(
        &self,
        spec: Spec,
        warnings: Vec<String>,
        out_dir: PathBuf,
        config: Config,
    ) -> Result<(), RunningJob> {
        let mut state = self.lock();
        if let Job::Running { id, stage, .. } = &*state {
            return Err(RunningJob {
                id: id.clone(),
                stage: *stage,
            });
        }
        *state = Job::Running {
            id: spec.id.clone(),
            stage: BuildStage::Preparing,
            started: Instant::now(),
            diagnostic: None,
        };
        drop(state);

        let events = self.clone();
        let finish = self.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                build::run_build(&spec, &warnings, &out_dir, &config, &mut |event| {
                    events.record(event);
                })
            })
            .await;

            match result {
                Ok(Ok(outcome)) => finish.finish(Job::Done(Box::new(outcome))),
                Ok(Err(error)) => finish.fail(error.to_string()),
                Err(join) if join.is_panic() => {
                    finish.fail("the build task panicked; see the server log".to_owned());
                }
                Err(join) => finish.fail(format!("the build task did not complete: {join}")),
            }
        });

        Ok(())
    }

    /// Records one progress event of the running build.
    fn record(&self, event: BuildEvent) {
        let mut state = self.lock();
        let Job::Running {
            stage, diagnostic, ..
        } = &mut *state
        else {
            return;
        };
        match event {
            BuildEvent::Stage(next) => *stage = next,
            BuildEvent::Diagnostic(tail) => *diagnostic = Some(tail),
        }
    }

    /// Records the terminal state of a successful build.
    fn finish(&self, job: Job) {
        *self.lock() = job;
    }

    /// Records the terminal state of a failed build, keeping the diagnostic
    /// tail the running job recorded.
    fn fail(&self, error: String) {
        let mut state = self.lock();
        let diagnostic = match &*state {
            Job::Running { diagnostic, .. } => diagnostic.clone(),
            _ => None,
        };
        *state = Job::Failed { error, diagnostic };
    }

    fn lock(&self) -> MutexGuard<'_, Job> {
        self.state
            .lock()
            .expect("the build job lock is never poisoned")
    }
}

/// Milliseconds since the job started.
fn elapsed_ms(started: &Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// `POST /api/build` — starts the one build job for a design.
///
/// The design is validated by the same Rust rules the CLI runs; only a valid
/// design starts a job, and the build needs no scratch spec file. The
/// response is the freshly running job; the UI polls `GET /api/build/status`
/// for the outcome.
pub(crate) async fn start(
    State(workspace): State<Workspace>,
    body: Bytes,
) -> Result<(StatusCode, Json<BuildStatusResponse>), ApiError> {
    let DesignRequest { design } = parse_body(&body)?;

    let spec = design.to_spec();
    let validation = spec.validate();
    if !validation.is_valid() {
        return Err(ApiError::invalid(
            "the design is not valid",
            validation.errors,
        ));
    }

    let id = spec.id.clone();
    let out_dir = workspace.build_dir(&id);
    workspace
        .jobs()
        .try_start(
            spec,
            validation.warnings,
            out_dir,
            workspace.config().clone(),
        )
        .map_err(|running| {
            ApiError::conflict(format!(
                "a build is already running for `{}` ({}); wait for it to finish",
                running.id,
                running.stage.as_str()
            ))
        })?;

    Ok((
        StatusCode::ACCEPTED,
        Json(BuildStatusResponse::Running {
            id,
            stage: BuildStage::Preparing,
            elapsed_ms: 0,
        }),
    ))
}

/// `GET /api/build/status` — the job state.
pub(crate) async fn status(State(workspace): State<Workspace>) -> Json<BuildStatusResponse> {
    let response = match workspace.jobs().snapshot() {
        JobSnapshot::Idle => BuildStatusResponse::Idle,
        JobSnapshot::Running {
            id,
            stage,
            elapsed_ms,
        } => BuildStatusResponse::Running {
            id,
            stage,
            elapsed_ms,
        },
        JobSnapshot::Done(outcome) => BuildStatusResponse::Done {
            outcome: Box::new(BuildOutcomeDto::from_outcome(&workspace, &outcome)),
        },
        JobSnapshot::Failed { error, diagnostic } => BuildStatusResponse::Failed {
            error,
            diagnostic: diagnostic.unwrap_or_default(),
        },
    };
    Json(response)
}
