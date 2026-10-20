//! `boxgen serve` API tests: the in-process router, no sockets (ticket #17).
//!
//! Every test builds [`boxgen::serve::app`] over a temporary workspace and
//! drives it with `tower::ServiceExt::oneshot`, so no port is bound and no
//! process is spawned. The wire schema these tests pin is documented in
//! `src/serve/dto.rs`.

use std::path::{Path, PathBuf};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use boxgen::config::Config;
use boxgen::serve::{Workspace, app};
use serde_json::Value;
use tower::ServiceExt;

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

#[tokio::test]
async fn health_reports_ok() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (status, body) = json(&app(workspace(temp.path())), "GET", "/api/health", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["system_version"], boxgen::spec::SYSTEM_VERSION);
}

/// Every frontend asset is embedded with its content type, and the index page
/// carries the import map that resolves the bare `three` specifier.
#[tokio::test]
async fn assets_are_embedded_with_their_content_types() {
    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));

    let (status, content_type, html) = send(&router, "GET", "/", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("text/html; charset=utf-8"));
    assert!(html.contains("boxgen serve"), "{html}");
    assert!(html.contains(r#"type="importmap""#), "{html}");
    assert!(
        html.contains(r#""three": "/assets/vendor/three.module.min.js""#),
        "{html}"
    );

    for (path, expected) in [
        ("/assets/styles.css", "text/css; charset=utf-8"),
        ("/assets/app.js", "text/javascript; charset=utf-8"),
        ("/assets/api.js", "text/javascript; charset=utf-8"),
        ("/assets/editor.js", "text/javascript; charset=utf-8"),
        ("/assets/panels.js", "text/javascript; charset=utf-8"),
        ("/assets/viewer3d.js", "text/javascript; charset=utf-8"),
        (
            "/assets/vendor/three.module.min.js",
            "text/javascript; charset=utf-8",
        ),
        (
            "/assets/vendor/three.core.min.js",
            "text/javascript; charset=utf-8",
        ),
        (
            "/assets/vendor/STLLoader.js",
            "text/javascript; charset=utf-8",
        ),
        (
            "/assets/vendor/LICENSE-three.md",
            "text/plain; charset=utf-8",
        ),
    ] {
        let (status, content_type, body) = send(&router, "GET", path, None).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(content_type.as_deref(), Some(expected), "{path}");
        assert!(!body.is_empty(), "{path}");
    }

    let (status, _, _) = send(&router, "GET", "/assets/missing.js", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The embedded module graph resolves: every relative import of every asset is
/// itself an embedded asset, our modules use only relative specifiers and the
/// bare `three`, and the vendored three module's own split
/// (`./three.core.min.js`) is served too.
#[tokio::test]
async fn every_module_import_resolves_to_an_embedded_asset() {
    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");

    // Our modules: relative imports resolve; the only bare specifier is
    // `three`, which the import map pins (asserted by the asset test).
    for module in ["app.js", "api.js", "editor.js", "panels.js", "viewer3d.js"] {
        let text = read_asset(&assets, module);
        for specifier in import_specifiers(&text) {
            if specifier == "three" {
                continue;
            }
            assert!(
                specifier.starts_with("./") || specifier.starts_with("../"),
                "{module}: unexpected import specifier {specifier:?}"
            );
            assert_asset_served(&router, module, &specifier).await;
        }
    }

    // Vendored sources: their relative imports (the release's module split)
    // must resolve; comments may mention addon specifiers, so bare ones are
    // ignored here.
    for module in [
        "vendor/STLLoader.js",
        "vendor/three.module.min.js",
        "vendor/three.core.min.js",
    ] {
        let text = read_asset(&assets, module);
        for specifier in import_specifiers(&text) {
            if !(specifier.starts_with("./") || specifier.starts_with("../")) {
                continue;
            }
            assert_asset_served(&router, module, &specifier).await;
        }
    }

    // The pinned release splits the core out; the vendored pair must stay in
    // step.
    let three = read_asset(&assets, "vendor/three.module.min.js");
    assert!(
        import_specifiers(&three)
            .iter()
            .any(|item| item == "./three.core.min.js"),
        "three.module.min.js must import the vendored ./three.core.min.js"
    );
}

fn read_asset(assets: &Path, module: &str) -> String {
    let path = assets.join(module);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

async fn assert_asset_served(router: &Router, module: &str, specifier: &str) {
    let url = resolve_asset_url(module, specifier);
    let (status, _, body) = send(router, "GET", &url, None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{module} imports {specifier} -> {url}"
    );
    assert!(!body.is_empty(), "{url} is empty");
}

/// Every `from "…"` / `import "…"` specifier in one module's source.
fn import_specifiers(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for keyword in ["from", "import"] {
        let mut rest = text;
        while let Some(index) = rest.find(keyword) {
            let after = &rest[index + keyword.len()..];
            let trimmed = after.trim_start();
            let quote = trimmed.chars().next();
            if let Some(quote @ ('"' | '\'')) = quote
                && let Some(end) = trimmed[1..].find(quote)
            {
                found.push(trimmed[1..1 + end].to_owned());
            }
            rest = after;
        }
    }
    found
}

/// Resolves one relative specifier against the importing module's URL.
fn resolve_asset_url(module: &str, specifier: &str) -> String {
    let mut parts = module.split('/').collect::<Vec<_>>();
    parts.pop();
    for segment in specifier.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/assets/{}", parts.join("/"))
}

/// Asserts a JSON number against a Rust constant.
fn assert_mm(body: &Value, pointer: &str, expected: f64) {
    let actual = body
        .pointer(pointer)
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("{pointer} is not a number: {body}"));
    assert!(
        (actual - expected).abs() < 1e-9,
        "{pointer}: expected {expected}, got {actual}"
    );
}

/// Asserts a JSON integer against a Rust constant.
fn assert_units(body: &Value, pointer: &str, expected: u32) {
    let actual = body
        .pointer(pointer)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("{pointer} is not an integer: {body}"));
    assert_eq!(actual, u64::from(expected), "{pointer}");
}

/// A fixture file's bytes.
fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// Writes a spec into the temp registry folder `boxes/<folder>/spec.yaml`.
fn write_box(root: &Path, folder: &str, text: &str) {
    let dir = root.join("boxes").join(folder);
    std::fs::create_dir_all(&dir).expect("create box folder");
    std::fs::write(dir.join("spec.yaml"), text).expect("write spec");
}

/// The smallest valid design: 1x1 3U, one named leaf, every wire key present.
fn minimal_design(id: &str) -> Value {
    serde_json::json!({
        "id": id,
        "footprint": [1, 1],
        "height_u": 3,
        "lid": false,
        "lid_ribs": false,
        "notes": Value::Null,
        "contents": Value::Null,
        "divisions": {"type": "leaf", "compartment": "Bits"},
    })
}

/// The panel is the standard's fixed values; every number must come from the
/// Rust constants, and the calibration reference must be the coupon record.
#[tokio::test]
async fn standard_panel_numbers_equal_the_rust_constants() {
    use boxgen::tree;

    let temp = tempfile::tempdir().expect("tempdir");
    let (status, body) = json(&app(workspace(temp.path())), "GET", "/api/standard", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["system_version"], boxgen::spec::SYSTEM_VERSION);

    assert_mm(&body, "/walls/wall_mm", tree::WALL_MM);
    assert_mm(&body, "/walls/base_zone_mm", tree::BASE_HEIGHT_MM);
    assert_mm(&body, "/walls/floor_mm", tree::FLOOR_MM);
    assert_mm(&body, "/walls/outer_radius_mm", tree::OUTER_RADIUS_MM);
    assert_mm(&body, "/walls/inner_radius_mm", tree::INNER_RADIUS_MM);
    assert_mm(&body, "/walls/floor_fillet_mm", tree::FLOOR_FILLET_MM);

    assert_mm(&body, "/dividers/thickness_mm", tree::DIVIDER_MM);
    assert_mm(
        &body,
        "/dividers/top_chamfer_mm",
        tree::DIVIDER_TOP_CHAMFER_MM,
    );
    assert_mm(&body, "/dividers/cove_mm", tree::DIVIDER_COVE_MM);
    assert_units(&body, "/dividers/min_leaf_mm", tree::MIN_LEAF_MM);
    assert_units(&body, "/dividers/warn_leaf_mm", tree::WARN_LEAF_MM);
    assert_units(&body, "/dividers/max_depth", tree::MAX_TREE_DEPTH);

    for (index, height_u) in tree::HEIGHTS_U.iter().enumerate() {
        let geometry = tree::BodyGeometry::new([1, 1], *height_u);
        assert_units(
            &body,
            &format!("/dividers/tops_mm/{index}/height_u"),
            *height_u,
        );
        assert_mm(
            &body,
            &format!("/dividers/tops_mm/{index}/divider_height_mm"),
            geometry.divider_height_mm,
        );
        assert_mm(
            &body,
            &format!("/dividers/tops_mm/{index}/top_mm"),
            tree::FLOOR_TOP_MM + geometry.divider_height_mm,
        );
    }

    assert_mm(&body, "/lid/plate_mm", tree::LID_PLATE_MM);
    assert_mm(&body, "/lid/skirt_mm", tree::LID_SKIRT_MM);
    assert_mm(&body, "/lid/clearance_per_side_mm", tree::LID_CLEARANCE_MM);
    assert_mm(&body, "/lid/corner_radius_mm", tree::LID_CORNER_RADIUS_MM);
    assert_mm(&body, "/lid/top_roundover_mm", tree::LID_TOP_ROUNDOVER_MM);
    assert_mm(&body, "/lid/bottom_chamfer_mm", tree::LID_SKIRT_CHAMFER_MM);
    assert_mm(
        &body,
        "/lid/finger_hole_diameter_mm",
        tree::LID_FINGER_HOLE_DIAMETER_MM,
    );
    assert_mm(
        &body,
        "/lid/finger_hole_edge_distance_mm",
        tree::LID_FINGER_HOLE_EDGE_DISTANCE_MM,
    );
    assert_mm(&body, "/lid/rib_interference_mm", tree::LID_RIB_MM);

    assert_mm(&body, "/gridfinity/grid_mm", tree::GRID_MM);
    assert_mm(&body, "/gridfinity/height_unit_mm", tree::HEIGHT_UNIT_MM);
    assert_mm(
        &body,
        "/gridfinity/clearance_per_side_mm",
        tree::OUTER_CLEARANCE_MM / 2.0,
    );
    assert_mm(
        &body,
        "/gridfinity/nominal_width_1u_mm",
        tree::GRID_MM - tree::OUTER_CLEARANCE_MM,
    );
    assert_mm(&body, "/gridfinity/base_height_mm", tree::BASE_HEIGHT_MM);
    assert_mm(&body, "/gridfinity/lip_height_mm", tree::LIP_HEIGHT_MM);

    assert_units(
        &body,
        "/limits/min_footprint_units",
        tree::MIN_FOOTPRINT_UNITS,
    );
    assert_units(
        &body,
        "/limits/max_footprint_units",
        tree::MAX_FOOTPRINT_UNITS,
    );
    assert_mm(&body, "/limits/max_outer_mm", tree::MAX_OUTER_MM);
    assert_units(&body, "/limits/default_height_u", tree::DEFAULT_HEIGHT_U);
    assert_mm(
        &body,
        "/limits/max_contents_kg",
        boxgen::spec::MAX_CONTENTS_KG,
    );
    assert_mm(&body, "/limits/max_mass_warning_g", tree::MAX_MASS_G);
    assert_mm(&body, "/limits/max_print_hours", tree::MAX_PRINT_HOURS);
    assert_eq!(
        body["limits"]["heights_u"],
        serde_json::json!(tree::HEIGHTS_U)
    );

    assert_mm(
        &body,
        "/contents/rigid/per_side_mm",
        boxgen::spec::CONTENTS_CLEARANCE_PER_SIDE_MM,
    );
    assert_mm(
        &body,
        "/contents/rigid/above_mm",
        boxgen::spec::CONTENTS_HEIGHT_CLEARANCE_MM,
    );
    assert_mm(
        &body,
        "/contents/soft/per_side_mm",
        boxgen::spec::CONTENTS_SOFT_CLEARANCE_PER_SIDE_MM,
    );
    assert_mm(
        &body,
        "/contents/soft/above_mm",
        boxgen::spec::CONTENTS_SOFT_HEIGHT_CLEARANCE_MM,
    );
    assert_mm(
        &body,
        "/contents/diagonal_margin_mm",
        boxgen::spec::CONTENTS_DIAGONAL_MARGIN_MM,
    );

    assert_mm(
        &body,
        "/material/density_g_per_cm3",
        tree::PETG_DENSITY_G_PER_CM3,
    );
    assert_mm(
        &body,
        "/material/effective_flow_mm3_per_s",
        tree::EFFECTIVE_FLOW_MM3_PER_S,
    );

    assert_eq!(body["calibration"]["coupon_id"], boxgen::build::COUPON_ID);
    assert_eq!(body["calibration"]["date"], "2026-10-20");
    assert!(
        body["calibration"]["material"]
            .as_str()
            .is_some_and(|material| material.contains("PETG Basic")),
        "{body}"
    );
    assert!(
        body["calibration"]["nozzle"]
            .as_str()
            .is_some_and(|nozzle| nozzle.contains("0.4 mm")),
        "{body}"
    );
    assert!(
        body["calibration"]["result"]
            .as_str()
            .is_some_and(|result| result.contains("approved")),
        "{body}"
    );
}

#[tokio::test]
async fn save_then_open_is_byte_stable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));
    let design = minimal_design("1x1-3u-bits-v1");

    let (status, saved) = json(
        &router,
        "POST",
        "/api/boxes/1x1-3u-bits-v1/save",
        Some(serde_json::json!({ "design": design })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");

    let (status, opened) = json(&router, "POST", "/api/boxes/1x1-3u-bits-v1/open", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(opened["text"], saved["text"]);
    assert_eq!(opened["design"], design);
    assert_eq!(opened["valid"], true);
    assert_eq!(saved["path"], "/host/boxgen/boxes/1x1-3u-bits-v1/spec.yaml");

    // The bytes on disk are exactly what save reported, and the CLI reads them
    // back as a valid spec.
    let on_disk = std::fs::read_to_string(temp.path().join("boxes/1x1-3u-bits-v1/spec.yaml"))
        .expect("saved spec file");
    assert_eq!(on_disk, saved["text"].as_str().expect("saved text"));
    let spec = boxgen::spec::parse(&on_disk).expect("saved spec parses");
    assert!(spec.validate().is_valid(), "{:?}", spec.validate().errors);

    // Saving the design that open returned is idempotent, byte for byte.
    let (status, again) = json(
        &router,
        "POST",
        "/api/boxes/1x1-3u-bits-v1/save",
        Some(serde_json::json!({ "design": opened["design"] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["text"], saved["text"]);
}

#[tokio::test]
async fn save_canonicalizes_notes_and_contents() {
    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));
    let mut design = minimal_design("1x1-3u-cables-v1");
    design["notes"] = serde_json::json!("Cables \"A\"\nand more");
    design["contents"] = serde_json::json!({
        "description": "Cables, adapters | power",
        "soft": true,
        "max_length_mm": 40.0,
        "max_width_mm": 25.0,
        "max_height_mm": 1.0,
        "weight_kg": 1.5,
    });

    let (status, saved) = json(
        &router,
        "POST",
        "/api/boxes/1x1-3u-cables-v1/save",
        Some(serde_json::json!({ "design": design })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");

    let (status, opened) = json(&router, "POST", "/api/boxes/1x1-3u-cables-v1/open", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(opened["design"], design);
    assert_eq!(opened["warnings"], serde_json::json!([]));

    // The free-text values survive the canonical quoting.
    let text = opened["text"].as_str().expect("text");
    assert!(
        text.contains("notes: \"Cables \\\"A\\\"\\nand more\""),
        "{text}"
    );
    let spec = boxgen::spec::parse(text).expect("saved spec parses");
    assert_eq!(spec.notes.as_deref(), Some("Cables \"A\"\nand more"));
}

#[tokio::test]
async fn save_rejects_invalid_designs_with_schema_paths() {
    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));

    // A split below the 12 mm minimum: a rule violation with a schema path.
    let design = serde_json::json!({
        "id": "4x4-6u-bad-v1",
        "footprint": [4, 4],
        "height_u": 6,
        "lid": false,
        "lid_ribs": false,
        "notes": Value::Null,
        "contents": Value::Null,
        "divisions": {
            "type": "split",
            "axis": "x",
            "at_mm": 8,
            "near": {"type": "leaf", "compartment": Value::Null},
            "far": {"type": "leaf", "compartment": Value::Null},
        },
    });
    let (status, body) = json(
        &router,
        "POST",
        "/api/boxes/4x4-6u-bad-v1/save",
        Some(serde_json::json!({ "design": design })),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let errors = body["errors"].as_array().expect("errors");
    assert!(
        errors.iter().any(|error| error
            .as_str()
            .is_some_and(|error| error.contains("divisions: at_mm 8"))),
        "{body}"
    );
    assert!(
        errors.iter().any(|error| error
            .as_str()
            .is_some_and(|error| error.contains("divisions.near: leaf 8.0 mm"))),
        "{body}"
    );
    assert!(!temp.path().join("boxes/4x4-6u-bad-v1/spec.yaml").exists());
}

/// Save follows the standard layout: a legacy folder name is not a registry
/// key, so saving through it is refused and the file on disk is untouched.
/// Open still works for the legacy folder; Save as creates `boxes/<id>/`.
#[tokio::test]
async fn save_refuses_a_legacy_folder_key() {
    let temp = tempfile::tempdir().expect("tempdir");
    let legacy = fixture("minimal.yaml");
    write_box(temp.path(), "Legacy Box", &legacy);
    let router = app(workspace(temp.path()));

    let (status, body) = json(
        &router,
        "POST",
        "/api/boxes/Legacy%20Box/save",
        Some(serde_json::json!({ "design": minimal_design("1x1-3u-bits-v1") })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("not a valid box id")),
        "{body}"
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("boxes/Legacy Box/spec.yaml")).unwrap(),
        legacy,
        "a refused save must not touch the file"
    );
}

#[tokio::test]
async fn save_rejects_unknown_wire_fields_and_mismatched_ids() {
    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));

    // The closed wire schema rejects a fixed architecture field.
    let mut design = minimal_design("1x1-3u-bits-v1");
    design["wall_thickness"] = serde_json::json!(2.0);
    let (status, body) = json(
        &router,
        "POST",
        "/api/boxes/1x1-3u-bits-v1/save",
        Some(serde_json::json!({ "design": design })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("wall_thickness")),
        "{body}"
    );

    // ... and inside the division tree too.
    let mut design = minimal_design("1x1-3u-bits-v1");
    design["divisions"] =
        serde_json::json!({"type": "leaf", "compartment": "Bits", "radius": 1.75});
    let (status, body) = json(
        &router,
        "POST",
        "/api/boxes/1x1-3u-bits-v1/save",
        Some(serde_json::json!({ "design": design })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("radius")),
        "{body}"
    );

    // The design id must be the registry key: renames are a non-goal.
    let design = minimal_design("1x1-3u-other-v1");
    let (status, body) = json(
        &router,
        "POST",
        "/api/boxes/1x1-3u-bits-v1/save",
        Some(serde_json::json!({ "design": design })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["errors"]
            .as_array()
            .is_some_and(|errors| errors.iter().any(|error| error
                .as_str()
                .is_some_and(|error| error.contains("1x1-3u-other-v1")))),
        "{body}"
    );
    assert!(!temp.path().join("boxes/1x1-3u-bits-v1/spec.yaml").exists());
}

#[tokio::test]
async fn boxes_list_reports_valid_and_broken_specs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let tree = fixture("4x4-6u-tree.yaml");
    write_box(temp.path(), "4x4-6U-tree-v1", &tree);
    write_box(temp.path(), "broken", "system: [\n");
    let router = app(workspace(temp.path()));

    let (status, body) = json(&router, "GET", "/api/boxes", None).await;
    assert_eq!(status, StatusCode::OK);
    let boxes = body["boxes"].as_array().expect("boxes");
    assert_eq!(boxes.len(), 2);

    let first = &boxes[0];
    assert_eq!(first["id"], "4x4-6U-tree-v1");
    assert_eq!(first["folder"], "4x4-6U-tree-v1");
    assert_eq!(first["footprint"], serde_json::json!([4, 4]));
    assert_eq!(first["height_u"], 6);
    assert_eq!(first["lid"], false);
    assert_eq!(first["lid_ribs"], false);
    assert_eq!(first["valid"], true);
    assert_eq!(first["errors"], serde_json::json!([]));
    assert_eq!(first["path"], "/host/boxgen/boxes/4x4-6U-tree-v1/spec.yaml");
    assert!(
        first["mtime"].as_u64().is_some_and(|mtime| mtime > 0),
        "{first}"
    );

    let broken = &boxes[1];
    assert_eq!(broken["id"], "broken");
    assert_eq!(broken["folder"], "broken");
    assert!(broken["footprint"].is_null(), "{broken}");
    assert!(broken["height_u"].is_null(), "{broken}");
    assert_eq!(broken["valid"], false);
    assert!(
        broken["errors"][0]
            .as_str()
            .is_some_and(|error| error.contains("YAML parse error")),
        "{broken}"
    );
}

#[tokio::test]
async fn an_empty_registry_lists_no_boxes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (status, body) = json(&app(workspace(temp.path())), "GET", "/api/boxes", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["boxes"], serde_json::json!([]));
}

#[tokio::test]
async fn open_returns_the_raw_text_and_design() {
    let temp = tempfile::tempdir().expect("tempdir");
    let tree = fixture("4x4-6u-tree.yaml");
    write_box(temp.path(), "4x4-6U-tree-v1", &tree);
    let router = app(workspace(temp.path()));

    let (status, body) = json(&router, "POST", "/api/boxes/4x4-6U-tree-v1/open", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["text"], tree);
    assert_eq!(body["valid"], true);
    assert_eq!(body["warnings"], serde_json::json!([]));
    assert_eq!(body["design"]["id"], "4x4-6U-tree-v1");
    assert_eq!(body["design"]["footprint"], serde_json::json!([4, 4]));
    assert_eq!(body["design"]["divisions"]["type"], "split");
    assert_eq!(body["design"]["divisions"]["axis"], "x");
    assert_eq!(body["design"]["divisions"]["at_mm"], 80);
    assert_eq!(
        body["design"]["divisions"]["near"]["near"]["compartment"],
        "A"
    );
    assert_eq!(
        body["design"]["divisions"]["far"]["far"]["compartment"],
        "D"
    );
}

#[tokio::test]
async fn open_an_unknown_box_is_not_found() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (status, body) = json(
        &app(workspace(temp.path())),
        "POST",
        "/api/boxes/nope/open",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("nope"))
    );
}

/// A broken spec is listed *and* openable: the editor shows the raw text and
/// the parse or rule errors, and the file is never lost. A folder whose name
/// is not a valid box id still resolves by folder name.
#[tokio::test]
async fn open_shows_errors_for_a_broken_spec() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_box(temp.path(), "broken-parse-v1", "system: [\n");
    write_box(
        temp.path(),
        "broken-rule-v1",
        "system: SYS-2.0\n\
         id: broken-rule-v1\n\
         footprint: [4, 4]\n\
         height_u: 6\n\
         lid: false\n\
         divisions:\n\
         \x20 axis: x\n\
         \x20 at_mm: 8\n\
         \x20 near: { compartment: \"A\" }\n\
         \x20 far: { compartment: \"B\" }\n",
    );
    write_box(temp.path(), "Legacy Box", &fixture("minimal.yaml"));
    let router = app(workspace(temp.path()));

    // A parse failure: the raw text plus the parse error, no design.
    let (status, body) = json(&router, "POST", "/api/boxes/broken-parse-v1/open", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["text"], "system: [\n");
    assert!(body["design"].is_null(), "{body}");
    assert_eq!(body["valid"], false);
    assert!(
        body["errors"][0]
            .as_str()
            .is_some_and(|error| error.contains("YAML parse error")),
        "{body}"
    );

    // A rule violation: the design still loads so the editor can fix it.
    let (status, body) = json(&router, "POST", "/api/boxes/broken-rule-v1/open", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["valid"], false);
    assert!(!body["design"].is_null(), "{body}");
    assert!(
        body["errors"]
            .as_array()
            .expect("errors")
            .iter()
            .any(|error| error
                .as_str()
                .is_some_and(|error| error.contains("divisions: at_mm 8"))),
        "{body}"
    );

    // A folder name that is not a valid box id resolves by folder name.
    let (status, body) = json(&router, "POST", "/api/boxes/Legacy%20Box/open", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["valid"], true);
    assert_eq!(body["design"]["id"], "1x1-3u-bits-v1");
}

/// The registry key can never escape `boxes/`: a percent-encoded separator is
/// not resolved as a path, so a spec outside the registry is unreachable.
#[tokio::test]
async fn open_never_leaves_the_registry() {
    let temp = tempfile::tempdir().expect("tempdir");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&outside).expect("outside folder");
    std::fs::write(
        outside.join("spec.yaml"),
        "system: SYS-2.0\nid: outside-v1\nfootprint: [1, 1]\nheight_u: 3\nlid: false\ndivisions: {}\n",
    )
    .expect("outside spec");

    // `%2F` decodes to `/` inside the `{id}` segment; `..` is not a valid id.
    let (status, _, text) = send(
        &app(workspace(temp.path())),
        "POST",
        "/api/boxes/..%2Foutside/open",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{text}");
    assert!(!text.contains("outside-v1"), "{text}");
}

/// The golden document: `tests/golden/4x4-6u-tree.plan.json`.
fn golden(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is not valid JSON: {error}", path.display()))
}

/// Asserts a rectangle's `[x_mm, y_mm, width_mm, depth_mm]`.
fn assert_rect(rect: &Value, expected: [f64; 4]) {
    let actual = [
        rect["x_mm"].as_f64().expect("x_mm"),
        rect["y_mm"].as_f64().expect("y_mm"),
        rect["width_mm"].as_f64().expect("width_mm"),
        rect["depth_mm"].as_f64().expect("depth_mm"),
    ];
    assert_eq!(actual, expected, "{rect}");
}

/// The plan is the golden tree's compartments, rectangles, and divider
/// faces, driven from the fixture through open — exactly what the 2D preview
/// draws. The whole response is pinned by the golden document, and the faces
/// the ticket names are spot-checked independently.
#[tokio::test]
async fn plan_matches_the_golden_tree() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_box(temp.path(), "4x4-6U-tree-v1", &fixture("4x4-6u-tree.yaml"));
    let router = app(workspace(temp.path()));

    let (status, opened) = json(&router, "POST", "/api/boxes/4x4-6U-tree-v1/open", None).await;
    assert_eq!(status, StatusCode::OK);

    let (status, plan) = json(
        &router,
        "POST",
        "/api/plan",
        Some(serde_json::json!({ "design": opened["design"] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(plan, golden("4x4-6u-tree.plan.json"));

    assert_eq!(plan["valid"], true);
    assert_eq!(plan["errors"], serde_json::json!([]));
    assert_eq!(plan["warnings"], serde_json::json!([]));

    // 4x4 6U: outer 167.5, interior 163.5 x 163.5 x 36.45, divider top 32.
    assert_mm(&plan, "/body/outer_width_mm", 167.5);
    assert_mm(&plan, "/body/interior_width_mm", 163.5);
    assert_mm(&plan, "/body/interior_height_mm", 36.45);
    assert_mm(&plan, "/body/divider_height_mm", 26.45);
    assert_mm(&plan, "/body/divider_top_mm", 32.0);

    // Compartments in near-before-far order, as absolute rectangles.
    let leaves = plan["leaves"].as_array().expect("leaves");
    assert_eq!(
        leaves
            .iter()
            .map(|leaf| leaf["compartment"].as_str().unwrap_or("?"))
            .collect::<Vec<_>>(),
        ["A", "B", "C", "D"]
    );
    assert_rect(&leaves[0], [0.0, 0.0, 80.0, 50.0]);
    assert_rect(&leaves[1], [0.0, 52.0, 80.0, 111.5]);
    assert_rect(&leaves[2], [82.0, 0.0, 40.0, 163.5]);
    assert_rect(&leaves[3], [124.0, 0.0, 39.5, 163.5]);

    // Divider faces: root x at 80, near y at 50, far x at 40 + 82 = 122.
    let dividers = plan["dividers"].as_array().expect("dividers");
    assert_eq!(dividers.len(), 3);
    assert_eq!(dividers[0]["axis"], "x");
    assert_rect(&dividers[0], [80.0, 0.0, 2.0, 163.5]);
    assert_eq!(dividers[1]["axis"], "y");
    assert_rect(&dividers[1], [0.0, 50.0, 80.0, 2.0]);
    assert_eq!(dividers[2]["axis"], "x");
    assert_rect(&dividers[2], [122.0, 0.0, 2.0, 163.5]);

    // Every divider carries its schema path and the parent region it splits,
    // so the editor can clamp a drag without duplicating the layout walk.
    assert_eq!(dividers[0]["path"], "divisions");
    assert_eq!(dividers[1]["path"], "divisions.near");
    assert_eq!(dividers[2]["path"], "divisions.far");
    assert_rect(&dividers[0]["parent"], [0.0, 0.0, 163.5, 163.5]);
    assert_rect(&dividers[1]["parent"], [0.0, 0.0, 80.0, 163.5]);
    assert_rect(&dividers[2]["parent"], [82.0, 0.0, 81.5, 163.5]);
}

#[tokio::test]
async fn plan_reports_invalid_designs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));

    // An out-of-range footprint: valid false, errors, and no layout.
    let design = serde_json::json!({
        "id": "5x5-6u-outside-v1",
        "footprint": [5, 5],
        "height_u": 6,
        "lid": false,
        "divisions": {"type": "leaf", "compartment": Value::Null},
    });
    let (status, plan) = json(
        &router,
        "POST",
        "/api/plan",
        Some(serde_json::json!({ "design": design })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(plan["valid"], false);
    assert_eq!(plan["leaves"], serde_json::json!([]));
    assert_eq!(plan["dividers"], serde_json::json!([]));
    assert!(
        plan["errors"][0]
            .as_str()
            .is_some_and(|error| error.contains("footprint[0]")),
        "{plan}"
    );

    // A tree rule violation still reports the layout and the schema path.
    let design = serde_json::json!({
        "id": "4x4-6u-bad-v1",
        "footprint": [4, 4],
        "height_u": 6,
        "lid": false,
        "divisions": {
            "type": "split",
            "axis": "x",
            "at_mm": 8,
            "near": {"type": "leaf", "compartment": "A"},
            "far": {"type": "leaf", "compartment": "B"},
        },
    });
    let (status, plan) = json(
        &router,
        "POST",
        "/api/plan",
        Some(serde_json::json!({ "design": design })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(plan["valid"], false);
    assert_eq!(plan["leaves"].as_array().expect("leaves").len(), 2);
    assert_rect(&plan["dividers"][0], [8.0, 0.0, 2.0, 163.5]);
    assert!(
        plan["errors"]
            .as_array()
            .expect("errors")
            .iter()
            .any(|error| error
                .as_str()
                .is_some_and(|error| error.contains("divisions: at_mm 8"))),
        "{plan}"
    );
}

#[tokio::test]
async fn plan_rejects_a_malformed_body() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (status, body) = json(
        &app(workspace(temp.path())),
        "POST",
        "/api/plan",
        Some(serde_json::json!({ "design": {"id": "x"} })),
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

/// The fit assistant must use exactly the spec's standard §8 contents rules:
/// for any item, a candidate is viable if and only if a spec with those
/// contents on that footprint and height produces no `contents.*` warnings.
/// The matrix runs every footprint and height for a set of rigid and soft
/// items, so the endpoint cannot re-implement the clearances.
#[tokio::test]
async fn fit_matches_the_spec_contents_cases() {
    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));

    let items = [
        (200.0, 120.0, 80.0, None, false),
        (60.0, 20.0, 8.0, Some(2.0), false),
        (200.0, 30.0, 10.0, Some(1.0), true),
        (500.0, 500.0, 500.0, None, true),
        (12.0, 12.0, 1.0, None, true),
        (40.0, 30.0, 7.0, Some(12.0), false),
    ];

    for &(length, width, height, weight, soft) in &items {
        let mut request = serde_json::json!({
            "length_mm": length,
            "width_mm": width,
            "height_mm": height,
            "soft": soft,
        });
        if let Some(weight) = weight {
            request["weight_kg"] = serde_json::json!(weight);
        }

        let (status, fit) = json(&router, "POST", "/api/fit", Some(request.clone())).await;
        assert_eq!(status, StatusCode::OK, "{request}");
        assert_eq!(fit["soft"], soft);
        assert_eq!(fit["weight_ok"], weight.is_none_or(|weight| weight <= 10.0));
        assert_mm(
            &fit,
            "/clearances/per_side_mm",
            if soft {
                boxgen::spec::CONTENTS_SOFT_CLEARANCE_PER_SIDE_MM
            } else {
                boxgen::spec::CONTENTS_CLEARANCE_PER_SIDE_MM
            },
        );
        assert_mm(
            &fit,
            "/clearances/height_clearance_mm",
            if soft {
                boxgen::spec::CONTENTS_SOFT_HEIGHT_CLEARANCE_MM
            } else {
                boxgen::spec::CONTENTS_HEIGHT_CLEARANCE_MM
            },
        );
        assert_mm(
            &fit,
            "/clearances/diagonal_margin_mm",
            boxgen::spec::CONTENTS_DIAGONAL_MARGIN_MM,
        );

        let viable = fit["viable"]
            .as_array()
            .expect("viable")
            .iter()
            .map(|candidate| {
                (
                    candidate["footprint"][0].as_u64().expect("n") as u32,
                    candidate["footprint"][1].as_u64().expect("m") as u32,
                    candidate["height_u"].as_u64().expect("height") as u32,
                )
            })
            .collect::<Vec<_>>();

        for n in 1..=4u32 {
            for m in 1..=4u32 {
                for height_u in [3u32, 6, 9, 12] {
                    let clean = contents_warning_free(&FitCase {
                        n,
                        m,
                        height_u,
                        length,
                        width,
                        height,
                        weight,
                        soft,
                    });
                    assert_eq!(
                        viable.contains(&(n, m, height_u)),
                        clean,
                        "item {request} on {n}x{m} {height_u}U"
                    );
                }
            }
        }
    }
}

/// One fit-case item on one candidate body.
struct FitCase {
    n: u32,
    m: u32,
    height_u: u32,
    length: f64,
    width: f64,
    height: f64,
    weight: Option<f64>,
    soft: bool,
}

/// Builds the equivalent spec and reports whether its *contents* checks pass.
fn contents_warning_free(case: &FitCase) -> bool {
    let FitCase {
        n,
        m,
        height_u,
        length,
        width,
        height,
        weight,
        soft,
    } = *case;
    let mut yaml = format!(
        "system: SYS-2.0\n\
         id: {n}x{m}-fit-v1\n\
         footprint: [{n}, {m}]\n\
         height_u: {height_u}\n\
         lid: false\n\
         contents:\n\
         \x20 soft: {soft}\n\
         \x20 max_length_mm: {length}\n\
         \x20 max_width_mm: {width}\n\
         \x20 max_height_mm: {height}\n"
    );
    if let Some(weight) = weight {
        yaml.push_str(&format!("  weight_kg: {weight}\n"));
    }
    yaml.push_str("divisions: {}\n");

    let spec = boxgen::spec::parse(&yaml)
        .unwrap_or_else(|error| panic!("fit case spec must parse ({error}):\n{yaml}"));
    spec.validate()
        .warnings
        .iter()
        .all(|warning| !warning.starts_with("contents."))
}

#[tokio::test]
async fn fit_orders_candidates_and_rejects_unusable_items() {
    let temp = tempfile::tempdir().expect("tempdir");
    let router = app(workspace(temp.path()));

    // A 60 x 20 x 8 mm rigid item fits the 1x1 3U body and larger ones; the
    // list is sorted by footprint area, then footprint, then height.
    let (status, fit) = json(
        &router,
        "POST",
        "/api/fit",
        Some(serde_json::json!({
            "length_mm": 60.0,
            "width_mm": 20.0,
            "height_mm": 8.0,
            "weight_kg": 2.0,
            "soft": false,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let viable = fit["viable"].as_array().expect("viable");
    assert!(!viable.is_empty());
    assert_eq!(
        viable[0],
        serde_json::json!({"footprint": [1, 2], "height_u": 3}),
        "the smallest viable combination comes first"
    );
    let areas = viable
        .iter()
        .map(|candidate| {
            candidate["footprint"][0].as_u64().unwrap()
                * candidate["footprint"][1].as_u64().unwrap()
        })
        .collect::<Vec<_>>();
    assert!(areas.windows(2).all(|pair| pair[0] <= pair[1]), "{fit}");

    for request in [
        serde_json::json!({"length_mm": 0.0, "width_mm": 20.0, "height_mm": 8.0}),
        serde_json::json!({"length_mm": 60.0, "width_mm": -1.0, "height_mm": 8.0}),
        serde_json::json!({"length_mm": 60.0, "width_mm": 20.0, "height_mm": 8.0, "weight_kg": -0.1}),
    ] {
        let (status, body) = json(&router, "POST", "/api/fit", Some(request.clone())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{request}");
        assert!(body["error"].as_str().is_some(), "{body}");
    }
}
