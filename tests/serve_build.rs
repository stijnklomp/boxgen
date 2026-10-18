//! `boxgen serve` build-path tests: jobs, artifacts, export, publish, index
//! (ticket #18).
//!
//! The endpoints are driven in process through the router
//! (`tower::ServiceExt::oneshot`) over a temporary workspace and the shared
//! fake `freecadcmd` launcher, so no FreeCAD is needed; one end-to-end case
//! runs the real pinned FreeCAD when `BOXGEN_REQUIRE_FREECAD=1`. The wire
//! schema these tests pin is documented in `src/serve/dto.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use boxgen::config::Config;
use boxgen::serve::{Workspace, app};
use serde_json::{Value, json};
use tower::ServiceExt;

mod common;

/// A workspace rooted at `root`; displayed paths come from `host_workdir`.
fn workspace(root: &Path) -> Workspace {
    Workspace::new(
        root,
        Config {
            ui_host_workdir: Some(PathBuf::from("/host/boxgen")),
            ..Config::default()
        },
    )
}

/// A workspace whose build runs the given fake launcher.
fn workspace_with(root: &Path, launcher: &Path) -> Workspace {
    Workspace::new(
        root,
        Config {
            freecadcmd: launcher.to_path_buf(),
            ui_host_workdir: Some(PathBuf::from("/host/boxgen")),
            ..Config::default()
        },
    )
}

/// Sends one request and returns the status, content type, and body text.
async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Option<String>, String) {
    let mut builder = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(serde_json::to_vec(&value).expect("request body serializes"))
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(builder.body(body).expect("request builds"))
        .await
        .expect("in-process router answers");
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body reads");
    (
        status,
        content_type,
        String::from_utf8(bytes.to_vec()).expect("response body is UTF-8"),
    )
}

/// Sends a request and parses the JSON body.
async fn json(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let (status, _, text) = send(app, method, uri, body).await;
    let value = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{method} {uri} returned non-JSON ({error}): {text}"));
    (status, value)
}

/// Polls `GET /api/build/status` until the job reaches a terminal state.
///
/// `attempts` bounds the wait at 25 ms per attempt.
async fn wait_for_build(app: &Router, attempts: u32) -> Value {
    for _ in 0..attempts {
        let (status, body) = json(app, "GET", "/api/build/status", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        match body["state"].as_str() {
            Some("done") | Some("failed") => return body,
            _ => tokio::time::sleep(Duration::from_millis(25)).await,
        }
    }
    panic!("the build job did not finish in time");
}

/// The wire design equal to `tests/fixtures/minimal-build.yaml`.
fn minimal_design() -> Value {
    json!({
        "id": "1x1-6U-minimal-v1",
        "footprint": [1, 1],
        "height_u": 6,
        "lid": false,
        "lid_ribs": false,
        "notes": Value::Null,
        "contents": Value::Null,
        "divisions": {"type": "leaf", "compartment": "Minimal"},
    })
}

/// The wire design equal to `tests/fixtures/2x4-6u-lid.yaml`.
fn lid_design() -> Value {
    json!({
        "id": "2x4-6U-lid-box-v1",
        "footprint": [2, 4],
        "height_u": 6,
        "lid": true,
        "lid_ribs": false,
        "notes": Value::Null,
        "contents": Value::Null,
        "divisions": {"type": "leaf", "compartment": "Lid box"},
    })
}

/// A complete, contract-shaped fake `report.json`, with or without the lid
/// sections.
fn fake_report(id: &str, lid: bool) -> String {
    let mut value = json!({
        "id": id,
        "dimensions": {"outer_x_mm": 41.5, "outer_y_mm": 41.5, "height_mm": 42.0},
        "interior_depth_mm": 33.25,
        "body": {
            "volume_mm3": 18397.5,
            "area_mm2": 1.0,
            "faces": 1,
            "solids": 1,
            "is_valid": true,
            "is_closed": true,
            "floor_fillet_mm": 2.0,
        },
        "stl": {"facets": 1, "is_solid": true, "volume_mm3": 1.0},
        "artifacts": {"box_step_bytes": 1, "body_stl_bytes": 1},
        "mass_g": 23.4,
        "print_time_h": 0.93,
        "warnings": [],
    });
    if lid {
        value["lid"] = json!({
            "volume_mm3": 2584.66,
            "area_mm2": 1.0,
            "faces": 1,
            "solids": 1,
            "is_valid": true,
            "is_closed": true,
            "roundover_mm": 1.0,
            "ribs": false,
        });
        value["lid_stl"] = json!({"facets": 1, "is_solid": true, "volume_mm3": 1.0});
        value["artifacts"]["lid_stl_bytes"] = json!(1);
    }
    serde_json::to_string(&value).expect("a JSON value serializes")
}

/// The shell body of a fake launcher: version check, an optional rendezvous
/// before the artifacts (bounded at about 10 s so a failing test cannot hang
/// it forever), then every artifact and the given report.
fn launcher_body(
    marker: Option<&Path>,
    release: Option<&Path>,
    report: &Path,
    lid: bool,
) -> String {
    let mut body = String::from(
        "if [ \"$1\" = \"--version\" ]; then echo 'FreeCAD 1.0.0 Revision:'; exit 0; fi\n\
         out=$(dirname \"$1\")\n",
    );
    if let (Some(marker), Some(release)) = (marker, release) {
        body.push_str(&format!(
            "touch '{}'\n\
             i=0\n\
             while [ ! -f '{}' ] && [ $i -lt 100 ]; do sleep 0.1; i=$((i+1)); done\n",
            marker.display(),
            release.display(),
        ));
    }
    body.push_str(
        "printf 'STEP' > \"$out/box.step\"\n\
         printf 'BODY' > \"$out/body.stl\"\n\
         printf 'FCSTD' > \"$out/box.FCStd\"\n",
    );
    if lid {
        body.push_str("printf 'LID' > \"$out/lid.stl\"\n");
    }
    body.push_str(&format!(
        "cp '{}' \"$out/report.json\"\nexit 0",
        report.display()
    ));
    body
}

/// A fake `freecadcmd` that writes every artifact for one report.
fn launcher(dir: &Path, report: &str, lid: bool) -> PathBuf {
    let report_path = dir.join("report-source.json");
    fs::write(&report_path, report).expect("write report source");
    common::fake_launcher(dir, &launcher_body(None, None, &report_path, lid))
}

/// A fake `freecadcmd` that touches `marker` and then blocks until `release`
/// exists, so a test can observe the running job.
fn rendezvous_launcher(
    dir: &Path,
    marker: &Path,
    release: &Path,
    report: &str,
    lid: bool,
) -> PathBuf {
    let report_path = dir.join("report-source.json");
    fs::write(&report_path, report).expect("write report source");
    common::fake_launcher(
        dir,
        &launcher_body(Some(marker), Some(release), &report_path, lid),
    )
}

/// Writes a spec into the temp registry folder `boxes/<folder>/spec.yaml`.
fn write_box(root: &Path, folder: &str, text: &str) {
    let dir = root.join("boxes").join(folder);
    fs::create_dir_all(&dir).expect("create box folder");
    fs::write(dir.join("spec.yaml"), text).expect("write spec");
}

/// The fixture file's bytes.
fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

#[tokio::test]
async fn build_status_is_idle_before_any_build() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (status, body) = json(
        &app(workspace(temp.path())),
        "GET",
        "/api/build/status",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"state": "idle"}));
}

/// One API-driven build: the job runs the one pipeline, renders the golden
/// script without any scratch spec file, and reports a structured outcome.
#[tokio::test]
async fn api_build_renders_the_golden_script_and_reports_the_outcome() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = launcher(temp.path(), &fake_report("1x1-6U-minimal-v1", false), false);
    let router = app(workspace_with(temp.path(), &launcher));

    let (status, started) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    assert_eq!(started["state"], "running");
    assert_eq!(started["id"], "1x1-6U-minimal-v1");
    assert_eq!(started["stage"], "preparing");
    assert_eq!(started["elapsed_ms"], 0);

    let body = wait_for_build(&router, 400).await;
    assert_eq!(body["state"], "done", "{body}");
    let outcome = &body["outcome"];
    assert_eq!(outcome["id"], "1x1-6U-minimal-v1");
    assert_eq!(outcome["out_dir"], "/host/boxgen/build/1x1-6U-minimal-v1");
    assert_eq!(outcome["report"]["body"]["solids"], 1);
    assert_eq!(outcome["report"]["body"]["floor_fillet_mm"], 2.0);
    assert_eq!(outcome["report"]["warnings"], json!([]));
    assert!(outcome["published_dir"].is_null(), "{outcome}");

    let artifacts = outcome["artifacts"].as_array().expect("artifacts");
    assert_eq!(
        artifacts
            .iter()
            .map(|artifact| artifact["name"].as_str().unwrap_or("?"))
            .collect::<Vec<_>>(),
        ["box.py", "box.step", "body.stl", "box.FCStd", "report.json"]
    );
    assert_eq!(
        artifacts[2]["path"],
        "/host/boxgen/build/1x1-6U-minimal-v1/body.stl"
    );
    assert!(
        artifacts[2]["bytes"]
            .as_u64()
            .is_some_and(|bytes| bytes > 0)
    );

    // The API-driven build renders the same generated script as the CLI and
    // the golden snapshot: one pipeline, no drift.
    let script = fs::read(temp.path().join("build/1x1-6U-minimal-v1/box.py")).expect("box.py");
    assert_eq!(script, include_bytes!("golden/minimal.box.py"));

    // Building an unsaved design never touches the registry.
    assert!(
        !temp.path().join("boxes").exists(),
        "the build must not create registry files"
    );
}

#[tokio::test]
async fn build_rejects_an_invalid_design_before_starting_the_job() {
    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));
    let mut design = minimal_design();
    design["footprint"] = json!([5, 5]);

    let (status, body) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": design})),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["errors"]
            .as_array()
            .expect("errors")
            .iter()
            .any(|error| error
                .as_str()
                .is_some_and(|error| error.contains("footprint[0]"))),
        "{body}"
    );

    let (_, status_body) = json(&router, "GET", "/api/build/status", None).await;
    assert_eq!(status_body, json!({"state": "idle"}));
    assert!(!temp.path().join("build").exists());
}

#[tokio::test]
async fn build_rejects_a_malformed_body() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (status, body) = json(
        &app(workspace(temp.path())),
        "POST",
        "/api/build",
        Some(json!({"design": {"id": "x"}})),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("missing field")),
        "{body}"
    );
}

/// One build at a time: the running job reports its stage and elapsed time, a
/// second POST is rejected clearly (no queue), and the next POST after the
/// job finishes is accepted.
#[tokio::test]
async fn one_build_at_a_time_and_no_queue() {
    let temp = tempfile::tempdir().expect("tempdir");
    let marker = temp.path().join("building");
    let release = temp.path().join("release");
    let launcher = rendezvous_launcher(
        temp.path(),
        &marker,
        &release,
        &fake_report("1x1-6U-minimal-v1", false),
        false,
    );
    let router = app(workspace_with(temp.path(), &launcher));

    let (status, started) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");

    // Wait until the launcher is actually running the generated script.
    for _ in 0..400 {
        if marker.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(marker.exists(), "the fake launcher never started");

    let (status, running) = json(&router, "GET", "/api/build/status", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(running["state"], "running", "{running}");
    assert_eq!(running["id"], "1x1-6U-minimal-v1");
    assert_eq!(running["stage"], "building");
    assert!(
        running["elapsed_ms"].as_u64().is_some(),
        "the running job must report elapsed time: {running}"
    );

    let (status, busy) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{busy}");
    assert!(
        busy["error"]
            .as_str()
            .is_some_and(|error| error.contains("already running")),
        "{busy}"
    );

    // Release the build; the job finishes and the next build is accepted.
    fs::write(&release, b"go").expect("release the fake launcher");
    let body = wait_for_build(&router, 400).await;
    assert_eq!(body["state"], "done", "{body}");

    let (status, restarted) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{restarted}");
    let body = wait_for_build(&router, 400).await;
    assert_eq!(body["state"], "done", "{body}");
}

/// A failed build carries the exit-contract message plus the raw `freecadcmd`
/// output tail, so the UI can show the diagnostics.
#[tokio::test]
async fn a_failed_build_reports_the_error_and_the_diagnostic_tail() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = common::fake_launcher(
        temp.path(),
        "if [ \"$1\" = \"--version\" ]; then echo 'FreeCAD 1.0.0 Revision:'; exit 0; fi\n\
         echo 'simulated geometry failure' >&2\n\
         exit 1",
    );
    let router = app(workspace_with(temp.path(), &launcher));

    let (status, started) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");

    let body = wait_for_build(&router, 400).await;
    assert_eq!(body["state"], "failed", "{body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("simulated geometry failure")),
        "{body}"
    );
    assert_eq!(body["diagnostic"], "simulated geometry failure");
}

/// The artifacts endpoint lists the build directory on disk with the host
/// paths the user can open in a slicer.
#[tokio::test]
async fn artifacts_lists_the_build_files_with_host_paths() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = launcher(temp.path(), &fake_report("1x1-6U-minimal-v1", false), false);
    let router = app(workspace_with(temp.path(), &launcher));

    let (status, _) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let body = wait_for_build(&router, 400).await;
    assert_eq!(body["state"], "done", "{body}");

    let (status, listing) = json(&router, "GET", "/api/artifacts/1x1-6U-minimal-v1", None).await;
    assert_eq!(status, StatusCode::OK, "{listing}");
    assert_eq!(listing["id"], "1x1-6U-minimal-v1");
    assert_eq!(listing["out_dir"], "/host/boxgen/build/1x1-6U-minimal-v1");

    let artifacts = listing["artifacts"].as_array().expect("artifacts");
    assert_eq!(
        artifacts
            .iter()
            .map(|artifact| artifact["name"].as_str().unwrap_or("?"))
            .collect::<Vec<_>>(),
        ["body.stl", "box.FCStd", "box.py", "box.step", "report.json"]
    );
    for artifact in artifacts {
        let name = artifact["name"].as_str().expect("name");
        assert_eq!(
            artifact["path"],
            format!("/host/boxgen/build/1x1-6U-minimal-v1/{name}")
        );
        assert!(
            artifact["bytes"].as_u64().is_some_and(|bytes| bytes > 0),
            "{artifact}"
        );
    }
    assert!(
        !listing.to_string().contains("cad"),
        "directories are not artifacts: {listing}"
    );
}

#[tokio::test]
async fn artifacts_of_an_unknown_build_is_not_found() {
    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));

    let (status, body) = json(&router, "GET", "/api/artifacts/1x1-6U-missing-v1", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("no build")),
        "{body}"
    );

    // A percent-encoded separator is not resolved as a path.
    let (status, _, _) = send(&router, "GET", "/api/artifacts/..%2Foutside", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The 3D preview loads the real meshes: `GET /api/artifacts/{id}/{name}`
/// serves the build directory's known files with their content types.
#[tokio::test]
async fn artifact_bytes_serve_the_meshes_with_their_content_types() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = launcher(temp.path(), &fake_report("2x4-6U-lid-box-v1", true), true);
    let router = app(workspace_with(temp.path(), &launcher));

    let (status, _) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": lid_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(wait_for_build(&router, 400).await["state"], "done");

    let (status, content_type, body) = send(
        &router,
        "GET",
        "/api/artifacts/2x4-6U-lid-box-v1/body.stl",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("model/stl"));
    assert_eq!(body, "BODY");

    let (status, content_type, body) = send(
        &router,
        "GET",
        "/api/artifacts/2x4-6U-lid-box-v1/lid.stl",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("model/stl"));
    assert_eq!(body, "LID");

    let (status, content_type, body) = send(
        &router,
        "GET",
        "/api/artifacts/2x4-6U-lid-box-v1/box.step",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("model/step"));
    assert_eq!(body, "STEP");

    let (status, content_type, body) = send(
        &router,
        "GET",
        "/api/artifacts/2x4-6U-lid-box-v1/report.json",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("application/json"));
    assert!(serde_json::from_str::<Value>(&body).is_ok(), "{body}");
}

/// Only the build directory's own generated files are reachable: an unknown
/// name, another folder's file, and a traversal attempt are all 404.
#[tokio::test]
async fn artifact_bytes_reject_unknown_names_and_builds() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = launcher(temp.path(), &fake_report("1x1-6U-minimal-v1", false), false);
    let router = app(workspace_with(temp.path(), &launcher));

    let (status, _) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(wait_for_build(&router, 400).await["state"], "done");

    for name in [
        "spec.yaml",
        "notes.md",
        "cad/shell.py",
        "..%2Fspec.yaml",
        "body.stl.bak",
    ] {
        let (status, _, text) = send(
            &router,
            "GET",
            &format!("/api/artifacts/1x1-6U-minimal-v1/{name}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{name}: {text}");
    }

    // An unknown build is 404 as well, and a missing artifact in a known
    // build is not a server error.
    let (status, _, _) = send(
        &router,
        "GET",
        "/api/artifacts/9x9-6U-nope-v1/body.stl",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    fs::remove_file(temp.path().join("build/1x1-6U-minimal-v1/body.stl"))
        .expect("remove the body mesh");
    let (status, body) = json(
        &router,
        "GET",
        "/api/artifacts/1x1-6U-minimal-v1/body.stl",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

/// Export writes the named copies with host paths, and a re-export overwrites
/// its own files without touching anything else in the folder.
#[tokio::test]
async fn export_writes_named_copies_and_overwrites_its_files_only() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = launcher(temp.path(), &fake_report("2x4-6U-lid-box-v1", true), true);
    let router = app(workspace_with(temp.path(), &launcher));

    let (status, _) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": lid_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let body = wait_for_build(&router, 400).await;
    assert_eq!(body["state"], "done", "{body}");

    let (status, exported) = json(
        &router,
        "POST",
        "/api/export",
        Some(json!({"base": "2x4-6U-lid-box-v1"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{exported}");
    assert_eq!(exported["base"], "2x4-6U-lid-box-v1");
    assert_eq!(
        exported["out_dir"],
        "/host/boxgen/exports/2x4-6U-lid-box-v1"
    );
    assert_eq!(
        exported["artifacts"]
            .as_array()
            .expect("artifacts")
            .iter()
            .map(|artifact| artifact["name"].as_str().unwrap_or("?"))
            .collect::<Vec<_>>(),
        [
            "2x4-6U-lid-box-v1-body.stl",
            "2x4-6U-lid-box-v1-lid.stl",
            "2x4-6U-lid-box-v1.step",
            "2x4-6U-lid-box-v1.FCStd",
        ]
    );
    assert_eq!(
        exported["artifacts"][0]["path"],
        "/host/boxgen/exports/2x4-6U-lid-box-v1/2x4-6U-lid-box-v1-body.stl"
    );

    let out = temp.path().join("exports/2x4-6U-lid-box-v1");
    assert_eq!(
        fs::read(out.join("2x4-6U-lid-box-v1-body.stl")).unwrap(),
        b"BODY"
    );
    assert_eq!(
        fs::read(out.join("2x4-6U-lid-box-v1-lid.stl")).unwrap(),
        b"LID"
    );
    assert_eq!(
        fs::read(out.join("2x4-6U-lid-box-v1.step")).unwrap(),
        b"STEP"
    );
    assert_eq!(
        fs::read(out.join("2x4-6U-lid-box-v1.FCStd")).unwrap(),
        b"FCSTD"
    );
    assert!(!out.join("box.py").exists(), "box.py is never exported");
    assert!(
        !out.join("report.json").exists(),
        "report.json is never exported"
    );

    // A human file in the export folder survives; the named copies are
    // overwritten with the new build's bytes.
    fs::write(out.join("keep.txt"), b"human notes").expect("keep.txt");
    fs::write(
        temp.path().join("build/2x4-6U-lid-box-v1/body.stl"),
        b"BODY-2",
    )
    .expect("replace the source body");
    let (status, again) = json(
        &router,
        "POST",
        "/api/export",
        Some(json!({"base": "2x4-6U-lid-box-v1"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(
        fs::read(out.join("2x4-6U-lid-box-v1-body.stl")).unwrap(),
        b"BODY-2"
    );
    assert_eq!(fs::read(out.join("keep.txt")).unwrap(), b"human notes");
}

/// A body-only build exports three named files: there is no lid copy.
#[tokio::test]
async fn export_of_a_body_only_build_has_no_lid_copy() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = launcher(temp.path(), &fake_report("1x1-6U-minimal-v1", false), false);
    let router = app(workspace_with(temp.path(), &launcher));

    let (status, _) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(wait_for_build(&router, 400).await["state"], "done");

    let (status, exported) = json(
        &router,
        "POST",
        "/api/export",
        Some(json!({"base": "1x1-6U-minimal-v1"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{exported}");
    assert_eq!(
        exported["artifacts"].as_array().expect("artifacts").len(),
        3
    );
    assert!(
        !temp
            .path()
            .join("exports/1x1-6U-minimal-v1/1x1-6U-minimal-v1-lid.stl")
            .exists()
    );
}

#[tokio::test]
async fn export_rejects_unknown_builds_and_unsafe_base_names() {
    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));

    let (status, body) = json(
        &router,
        "POST",
        "/api/export",
        Some(json!({"base": "1x1-6U-missing-v1"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // The base names both the build and the export folder; a separator or a
    // parent step can never leave the workspace.
    for base in ["../escape", "a/b", "..", ""] {
        let (status, body) =
            json(&router, "POST", "/api/export", Some(json!({"base": base}))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{base}: {body}");
    }
    assert!(!temp.path().join("escape").exists());
    assert!(!temp.path().join("exports").exists());
}

/// An interrupted build (missing artifacts) is a state conflict, not a server
/// error, and nothing is exported.
#[tokio::test]
async fn export_of_an_incomplete_build_is_a_conflict() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = launcher(temp.path(), &fake_report("1x1-6U-minimal-v1", false), false);
    let router = app(workspace_with(temp.path(), &launcher));

    let (status, _) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(wait_for_build(&router, 400).await["state"], "done");

    fs::remove_file(temp.path().join("build/1x1-6U-minimal-v1/body.stl"))
        .expect("remove the body mesh");

    let (status, body) = json(
        &router,
        "POST",
        "/api/export",
        Some(json!({"base": "1x1-6U-minimal-v1"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("body.stl")),
        "{body}"
    );
    assert!(
        !temp
            .path()
            .join("exports/1x1-6U-minimal-v1/1x1-6U-minimal-v1.step")
            .exists(),
        "nothing may be exported from an incomplete build"
    );
}

/// Publish copies the verified build into the registry folder that holds the
/// spec, with every `--publish` rule intact.
#[tokio::test]
async fn publish_copies_the_verified_build_into_the_registry() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = launcher(temp.path(), &fake_report("1x1-6U-minimal-v1", false), false);
    let router = app(workspace_with(temp.path(), &launcher));
    let spec = fixture("minimal-build.yaml");
    write_box(temp.path(), "1x1-6U-minimal-v1", &spec);

    let (status, _) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(wait_for_build(&router, 400).await["state"], "done");

    let (status, published) = json(
        &router,
        "POST",
        "/api/publish",
        Some(json!({"id": "1x1-6U-minimal-v1"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    assert_eq!(published["id"], "1x1-6U-minimal-v1");
    assert_eq!(
        published["published_dir"],
        "/host/boxgen/boxes/1x1-6U-minimal-v1"
    );

    let folder = temp.path().join("boxes/1x1-6U-minimal-v1");
    assert_eq!(
        names(&folder),
        ["body.stl", "box.step", "notes.md", "spec.yaml"]
    );
    assert_eq!(fs::read(folder.join("body.stl")).unwrap(), b"BODY");
    assert_eq!(fs::read(folder.join("box.step")).unwrap(), b"STEP");
    assert_eq!(
        fs::read_to_string(folder.join("spec.yaml")).unwrap(),
        spec,
        "publish never rewrites the spec"
    );
    let notes = fs::read_to_string(folder.join("notes.md")).expect("notes.md");
    assert!(notes.starts_with("# 1x1-6U-minimal-v1\n"), "{notes}");
    assert!(!folder.join("box.FCStd").exists());
    assert!(!folder.join("report.json").exists());
    assert!(!folder.join("box.py").exists());
    assert!(!folder.join("cad").exists());
}

/// The human print record wins: publish never overwrites an existing
/// `notes.md`.
#[tokio::test]
async fn publish_never_overwrites_an_existing_notes_md() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = launcher(temp.path(), &fake_report("1x1-6U-minimal-v1", false), false);
    let router = app(workspace_with(temp.path(), &launcher));
    write_box(
        temp.path(),
        "1x1-6U-minimal-v1",
        &fixture("minimal-build.yaml"),
    );
    fs::write(
        temp.path().join("boxes/1x1-6U-minimal-v1/notes.md"),
        "Printed: 2026-01-01\nFilament spool/lot: PETG-7\n",
    )
    .expect("handwritten notes");

    let (status, _) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(wait_for_build(&router, 400).await["state"], "done");

    let (status, published) = json(
        &router,
        "POST",
        "/api/publish",
        Some(json!({"id": "1x1-6U-minimal-v1"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    assert_eq!(
        fs::read_to_string(temp.path().join("boxes/1x1-6U-minimal-v1/notes.md")).unwrap(),
        "Printed: 2026-01-01\nFilament spool/lot: PETG-7\n"
    );
}

/// Every artifact is verified before the first copy: an incomplete build
/// leaves the box folder untouched.
#[tokio::test]
async fn publish_verifies_every_artifact_before_the_first_copy() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = launcher(temp.path(), &fake_report("1x1-6U-minimal-v1", false), false);
    let router = app(workspace_with(temp.path(), &launcher));
    write_box(
        temp.path(),
        "1x1-6U-minimal-v1",
        &fixture("minimal-build.yaml"),
    );

    let (status, _) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(wait_for_build(&router, 400).await["state"], "done");

    fs::remove_file(temp.path().join("build/1x1-6U-minimal-v1/box.step"))
        .expect("remove the STEP export");

    let (status, body) = json(
        &router,
        "POST",
        "/api/publish",
        Some(json!({"id": "1x1-6U-minimal-v1"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("box.step")),
        "{body}"
    );
    assert_eq!(
        names(&temp.path().join("boxes/1x1-6U-minimal-v1")),
        ["spec.yaml"],
        "a failed publish must not copy anything"
    );
}

/// Publish follows the CLI rule: the target is `boxes/<id>`, so a registry
/// folder with a legacy name is refused even when the spec inside it matches.
/// Open and save still tolerate the legacy name; publish never does.
#[tokio::test]
async fn publish_refuses_a_legacy_folder_name() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = launcher(temp.path(), &fake_report("1x1-6U-minimal-v1", false), false);
    let router = app(workspace_with(temp.path(), &launcher));
    write_box(temp.path(), "Legacy Box", &fixture("minimal-build.yaml"));

    let (status, _) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(wait_for_build(&router, 400).await["state"], "done");

    let (status, body) = json(
        &router,
        "POST",
        "/api/publish",
        Some(json!({"id": "1x1-6U-minimal-v1"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("named after")),
        "{body}"
    );
    assert_eq!(
        names(&temp.path().join("boxes/Legacy Box")),
        ["spec.yaml"],
        "a refused publish must not copy anything"
    );
}

/// The folder may be named after the request id but hold another spec id;
/// publish refuses that too, so it never writes a box under a mismatched id.
#[tokio::test]
async fn publish_refuses_a_spec_id_that_differs_from_the_request() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = launcher(temp.path(), &fake_report("1x1-6U-minimal-v1", false), false);
    let router = app(workspace_with(temp.path(), &launcher));
    let spec = fixture("minimal-build.yaml").replace("1x1-6U-minimal-v1", "1x1-6U-other-v1");
    write_box(temp.path(), "1x1-6U-minimal-v1", &spec);

    let (status, _) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(wait_for_build(&router, 400).await["state"], "done");

    let (status, body) = json(
        &router,
        "POST",
        "/api/publish",
        Some(json!({"id": "1x1-6U-minimal-v1"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("1x1-6U-other-v1")),
        "{body}"
    );
    assert_eq!(
        names(&temp.path().join("boxes/1x1-6U-minimal-v1")),
        ["spec.yaml"],
        "a refused publish must not copy anything"
    );
}

#[tokio::test]
async fn publish_rejects_unknown_boxes_and_missing_builds() {
    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));

    let (status, body) = json(
        &router,
        "POST",
        "/api/publish",
        Some(json!({"id": "1x1-6U-missing-v1"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // A saved box without a build cannot publish either.
    write_box(
        temp.path(),
        "1x1-6U-minimal-v1",
        &fixture("minimal-build.yaml"),
    );
    let (status, body) = json(
        &router,
        "POST",
        "/api/publish",
        Some(json!({"id": "1x1-6U-minimal-v1"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("no build")),
        "{body}"
    );
}

/// `POST /api/index` writes the same bytes as `boxgen index` for the same
/// registry.
#[tokio::test]
async fn index_matches_the_cli_byte_for_byte() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cli_registry = temp.path().join("cli-boxes");
    let workspace_root = temp.path().join("ui");
    let spec = fixture("4x4-6u-tree.yaml");
    for folder in [
        cli_registry.join("4x4-6U-tree-v1"),
        workspace_root.join("boxes/4x4-6U-tree-v1"),
    ] {
        fs::create_dir_all(&folder).expect("box folder");
        fs::write(folder.join("spec.yaml"), &spec).expect("spec");
        fs::write(folder.join("notes.md"), "Printed: 2026-10-17\n").expect("notes");
    }

    let output = common::boxgen()
        .args(["index", "--boxes"])
        .arg(&cli_registry)
        .output()
        .expect("boxgen index runs");
    common::assert_success(&output);

    let router = app(workspace(&workspace_root));

    let (status, body) = json(&router, "POST", "/api/index", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["path"], "/host/boxgen/boxes/INDEX.md");
    assert_eq!(body["boxes"], 1);

    let cli_index = fs::read(cli_registry.join("INDEX.md")).expect("CLI INDEX.md");
    let api_index = fs::read(workspace_root.join("boxes/INDEX.md")).expect("API INDEX.md");
    assert_eq!(
        api_index, cli_index,
        "the API index must match the CLI bytes"
    );
    assert!(String::from_utf8_lossy(&api_index).contains("| 4x4-6U-tree-v1 | 4 × 4 | 6U |"));
}

/// The strict all-or-nothing scan still applies, and a failed run leaves an
/// existing index untouched.
#[tokio::test]
async fn index_rejects_an_invalid_registry_without_touching_the_index() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_box(temp.path(), "broken", "system: [\n");
    let index_path = temp.path().join("boxes/INDEX.md");
    fs::write(&index_path, "# Box Index\n\nsentinel\n").expect("existing index");
    let router = app(workspace(temp.path()));

    let (status, body) = json(&router, "POST", "/api/index", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("broken")),
        "{body}"
    );
    assert_eq!(
        fs::read_to_string(&index_path).unwrap(),
        "# Box Index\n\nsentinel\n"
    );
}

/// A fresh workspace has no registry directory yet (git cannot track an empty
/// one); the button writes the committed empty index instead of failing, and
/// the bytes still match `boxgen index`.
#[tokio::test]
async fn index_of_a_fresh_workspace_writes_the_committed_empty_index() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cli_registry = temp.path().join("cli-boxes");
    fs::create_dir_all(&cli_registry).expect("empty CLI registry");
    let output = common::boxgen()
        .args(["index", "--boxes"])
        .arg(&cli_registry)
        .output()
        .expect("boxgen index runs");
    common::assert_success(&output);

    let workspace_root = temp.path().join("ui");
    fs::create_dir_all(&workspace_root).expect("fresh workspace");
    assert!(!workspace_root.join("boxes").exists());
    let router = app(workspace(&workspace_root));

    let (status, body) = json(&router, "POST", "/api/index", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["boxes"], 0);
    assert_eq!(body["path"], "/host/boxgen/boxes/INDEX.md");

    let api_index = fs::read(workspace_root.join("boxes/INDEX.md")).expect("API INDEX.md");
    let cli_index = fs::read(cli_registry.join("INDEX.md")).expect("CLI INDEX.md");
    assert_eq!(
        api_index, cli_index,
        "the empty index must match the CLI bytes"
    );
    assert!(String::from_utf8_lossy(&api_index).contains("No boxes recorded yet."));
}

/// The names in a directory, sorted; test scaffolding for folder contents.
fn names(dir: &Path) -> Vec<String> {
    let mut names = fs::read_dir(dir)
        .expect("read directory")
        .map(|entry| {
            entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// The CLI summary the API replaces must stay byte-identical: the whole
/// stdout block, through the process boundary, with the fake launcher.
#[test]
fn the_cli_build_summary_is_byte_identical_to_the_pinned_block() {
    let temp = tempfile::tempdir().expect("tempdir");
    let launcher = launcher(temp.path(), &fake_report("1x1-6U-minimal-v1", false), false);
    let out = temp.path().join("out");

    let output = common::boxgen()
        .args(["build"])
        .arg(common::fixture("minimal-build.yaml"))
        .arg("--out")
        .arg(&out)
        .env("BOXGEN_FREECADCMD", &launcher)
        .output()
        .expect("boxgen build runs");
    common::assert_success(&output);

    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!(
            "id: 1x1-6U-minimal-v1\n\
             output: {out}\n\
             outer: 41.5 x 41.5 x 42.0 mm\n\
             interior depth: 33.25 mm\n\
             body: 18397.5 mm3, 1 faces, 1.0 mm2 surface\n\
             floor fillet: 2.0 mm (standard 2.0, fallback 1.0)\n\
             stl: 1 facets, 1.0 mm3, watertight\n\
             mass: 23.4 g (body only; PETG 1.27 g/cm3)\n\
             print time: 0.93 h (at 5.5 mm3/s effective flow)\n\
             warnings: none\n\
             artifacts: box.step (1 bytes), body.stl (1 bytes)\n",
            out = out.display()
        )
    );
}

/// The real pinned FreeCAD, end to end: the API build produces the contract
/// artifacts and the measured report, and export copies the real print files.
///
/// Skipped natively; mandatory with `BOXGEN_REQUIRE_FREECAD=1` (the gate
/// image), exactly like the other geometry checks.
#[tokio::test]
async fn api_build_end_to_end_with_real_freecad() {
    if !common::freecad_required() {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));

    let (status, started) = json(
        &router,
        "POST",
        "/api/build",
        Some(json!({"design": minimal_design()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");

    let body = wait_for_build(&router, 2400).await;
    assert_eq!(body["state"], "done", "{body}");
    let outcome = &body["outcome"];
    assert_eq!(outcome["out_dir"], "/host/boxgen/build/1x1-6U-minimal-v1");
    assert_eq!(outcome["report"]["id"], "1x1-6U-minimal-v1");
    assert_eq!(outcome["report"]["body"]["solids"], 1);
    assert_eq!(outcome["report"]["body"]["is_valid"], true);
    assert_eq!(outcome["report"]["body"]["is_closed"], true);
    assert_eq!(outcome["report"]["body"]["floor_fillet_mm"], 2.0);
    assert_eq!(outcome["report"]["stl"]["is_solid"], true);
    assert_eq!(outcome["report"]["warnings"], json!([]));
    let volume = outcome["report"]["body"]["volume_mm3"]
        .as_f64()
        .expect("body volume");
    assert!(
        (volume - 18397.5).abs() <= 18397.5 * 0.03,
        "the 1x1 6U body volume must be about 18397.5 mm3, measured {volume}"
    );

    let (status, exported) = json(
        &router,
        "POST",
        "/api/export",
        Some(json!({"base": "1x1-6U-minimal-v1"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{exported}");
    let artifacts = exported["artifacts"].as_array().expect("artifacts");
    assert_eq!(artifacts.len(), 3);
    for artifact in artifacts {
        assert!(
            artifact["bytes"].as_u64().is_some_and(|bytes| bytes > 0),
            "{artifact}"
        );
    }
    let exported_body = temp
        .path()
        .join("exports/1x1-6U-minimal-v1/1x1-6U-minimal-v1-body.stl");
    assert!(
        fs::metadata(&exported_body)
            .expect("exported body.stl")
            .len()
            > 0
    );
}
