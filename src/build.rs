//! Build orchestration and `freecadcmd` invocation.
//!
//! Contract: `docs/generator.md` §5 (CLI contract, artifacts, publish) and §6
//! (FreeCAD master contract). `build` validates a spec, writes
//! `build/<id>/box.py`, runs it headless through `freecadcmd`, and verifies
//! `box.FCStd`, `box.step`, `body.stl`, and `report.json`.
//!
//! The division tree is embedded in the generated script (`node_json`) and
//! compiled by the Python master: every split becomes a divider fused into
//! the body (ticket #8). Publish (#11) copies the accepted artifacts into the
//! spec's box folder after a successful build. The coupon (#10) builds the
//! one internal calibration spec through the same [`run_build`] pipeline and
//! prints the calibration reminder; it never parses a user spec.
//!
//! No geometry is implemented in Rust. This module copies the Python master
//! into the build directory, renders the generated script, runs FreeCAD, and
//! checks the artifacts FreeCAD produced. `freecadcmd` exits 0 even when the
//! script raises, so a successful exit status alone is never trusted.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::{Config, ENV_FREECAD_VERSION, ENV_FREECADCMD};
use crate::error::BoxgenError;
use crate::spec::{self, Contents, Spec};
use crate::tree::{self, Node, fmt_mm};

/// Files the generated script and the build overwrite on every run.
///
/// `lid.stl` is boxgen-owned: clearing it keeps a stale lid from a previous
/// run out of a body-only build directory.
const GENERATED_ARTIFACTS: [&str; 6] = [
    "box.py",
    "box.step",
    "body.stl",
    "lid.stl",
    "box.FCStd",
    "report.json",
];

/// Artifacts that must exist and be non-empty after `freecadcmd` returns.
///
/// `box.py` is written by Rust and is not proof that FreeCAD ran; every other
/// file is produced by the Python master. `lid.stl` additionally joins this
/// list for a lid spec (ticket #9).
const BODY_OUTPUTS: [&str; 4] = ["report.json", "box.step", "body.stl", "box.FCStd"];

/// The artifacts `freecadcmd` must produce for a spec.
fn required_outputs(lid: bool) -> Vec<&'static str> {
    let mut outputs = BODY_OUTPUTS.to_vec();
    if lid {
        outputs.push("lid.stl");
    }
    outputs
}

/// Python files and the license copied from the repository's `cad/`.
const CAD_LICENSE: &str = "LICENSE-LGPL-2.1";

/// Builds one spec and reports the artifacts through stdout.
pub fn build(
    spec_path: &Path,
    out: Option<&Path>,
    publish: bool,
    config: &Config,
) -> Result<(), BoxgenError> {
    let spec = spec::load(spec_path)?;
    let validation = spec.validate();

    if !validation.is_valid() {
        return Err(spec::invalid_spec(spec_path, &validation.errors));
    }

    // Publishing targets the spec's own box folder. Resolve it before the build
    // so a spec outside the boxes/<id>/ layout fails fast and cheaply; nothing
    // is copied until the build has succeeded and every artifact verified.
    let publish_target = publish
        .then(|| publish_target(spec_path, &spec.id))
        .transpose()?;

    let out_dir = out.map_or_else(|| PathBuf::from("build").join(&spec.id), Path::to_path_buf);
    run_build(&spec, &validation.warnings, &out_dir, config)?;

    if let Some(target) = publish_target {
        publish_artifacts(&spec, &out_dir, &target)?;
        println!("published: {}", target.display());
    }

    Ok(())
}

/// The coupon id, internal and fixed by this module.
///
/// The coupon is not a user box, so the user-facing id rule (kebab-case or
/// `<N>x<M>-<height>U-<slug>-<rev>`) does not apply and `boxgen coupon` never
/// runs the user-spec validator. The id is still the report id and the name
/// the summary prints.
pub const COUPON_ID: &str = "coupon-1x1-6U-v1";

/// The default coupon build directory: where the human finds it.
pub const COUPON_OUT_DIR: &str = "build/coupon";

/// The one internal coupon spec (ticket #10, calibration section 2).
///
/// 1 x 1, 6U, ribbed friction lid, a single-leaf division tree, and the
/// internal `coupon: true` flag that makes the master fuse the calibration
/// T-divider into the shell. Built in code because the closed user schema
/// must never accept a `coupon` field.
pub fn coupon_spec() -> Spec {
    Spec {
        system: spec::SYSTEM_VERSION.to_owned(),
        id: COUPON_ID.to_owned(),
        footprint: [1, 1],
        height_u: 6,
        lid: true,
        lid_ribs: true,
        notes: None,
        contents: None,
        divisions: Node::leaf(Some("Coupon")),
        coupon: true,
    }
}

/// Generates the calibration coupon through the normal build path.
///
/// Calibration section 2 fixes the coupon; this command builds the internal
/// spec through the same [`run_build`] pipeline as `build` (render `box.py`,
/// run `freecadcmd`, verify artifacts and `report.json`, print the summary),
/// defaulting to [`COUPON_OUT_DIR`], then prints the calibration reminder.
pub fn coupon(out: Option<&Path>, config: &Config) -> Result<(), BoxgenError> {
    let spec = coupon_spec();
    let out_dir = out.map_or_else(|| PathBuf::from(COUPON_OUT_DIR), Path::to_path_buf);

    // No validation warnings: the coupon is fixed and warning-free, and the
    // user-spec validator (including the user id rule) does not apply.
    run_build(&spec, &[], &out_dir, config)?;

    println!();
    println!("calibration: print two coupons and one lid in the production recipe");
    println!("calibration: run the nine tests in docs/calibration.md and record the result there");

    Ok(())
}

/// Writes and verifies one build directory for an already validated spec.
///
/// This is the one orchestration shared by `build` and `coupon`: clear the
/// owned artifacts, copy the master, render `box.py`, run the pinned
/// `freecadcmd`, verify every artifact and the report invariants, merge the
/// given warnings, rewrite `report.json` deterministically, and print the
/// summary.
fn run_build(
    spec: &Spec,
    warnings: &[String],
    out_dir: &Path,
    config: &Config,
) -> Result<(), BoxgenError> {
    prepare_out_dir(out_dir)?;

    let cad_source = repo_cad_dir()?;
    copy_cad(&cad_source, &out_dir.join("cad"))?;
    write_file(&out_dir.join("box.py"), &render_box_py(spec))?;

    check_freecad(config)?;

    let script = std::path::absolute(out_dir.join("box.py")).map_err(|error| BoxgenError::Io {
        message: format!("cannot resolve the generated script path: {error}"),
    })?;
    let output = Command::new(&config.freecadcmd)
        .arg(&script)
        .current_dir(out_dir)
        .output()
        .map_err(|error| BoxgenError::Environment {
            message: format!(
                "cannot run `{}`: {error}; set {ENV_FREECADCMD} or install FreeCAD",
                config.freecadcmd.display()
            ),
        })?;

    // FreeCAD resets `sys.dont_write_bytecode`, so interpreter caches appear
    // next to the script and the master. They are not artifacts.
    remove_runtime_caches(out_dir);

    if !output.status.success() {
        return Err(BoxgenError::Build {
            id: spec.id.clone(),
            message: format!(
                "freecadcmd exited with {}:\n{}",
                output.status,
                diagnostic_tail(&output.stderr, &output.stdout)
            ),
        });
    }

    let mut report_json = read_report(out_dir, spec)?;
    merge_warnings(&mut report_json, warnings, &spec.id)?;
    let report: BuildReport =
        serde_json::from_value(report_json.clone()).map_err(|error| BoxgenError::Build {
            id: spec.id.clone(),
            message: format!("report.json does not match the build report schema: {error}"),
        })?;
    validate_report(&report, spec)?;
    write_report(&out_dir.join("report.json"), &report_json)?;
    remove_backups(out_dir);

    print_summary(out_dir, &report);

    Ok(())
}

/// Resolves the box folder a `--publish` run writes into.
///
/// The spec is the source of truth in its box folder, so the target is the
/// spec file's parent directory and that directory must be named after the
/// spec id: `boxes/<id>/spec.yaml`. A mismatch is a usage error; boxgen never
/// invents a second location.
fn publish_target(spec_path: &Path, id: &str) -> Result<PathBuf, BoxgenError> {
    let parent = spec_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());

    match parent
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
    {
        Some(name) if name == id => Ok(parent.expect("a named parent exists").to_path_buf()),
        _ => Err(BoxgenError::Usage {
            message: format!(
                "cannot publish `{}`: the spec must live at `boxes/<id>/spec.yaml`, \
                 with the parent directory named after its id `{id}`",
                spec_path.display()
            ),
        }),
    }
}

/// Copies the accepted print files into the box folder.
///
/// Every required artifact is verified before the first copy, so a missing or
/// empty file fails the publish and leaves the target untouched. `spec.yaml` is
/// never rewritten, `notes.md` is never overwritten, and `box.FCStd`,
/// `report.json`, `box.py`, and `cad/` are never published.
fn publish_artifacts(spec: &Spec, out_dir: &Path, target: &Path) -> Result<(), BoxgenError> {
    let notes = out_dir.join("notes.md");
    if !notes.exists() {
        write_file(&notes, &render_notes(spec))?;
    }

    let mut required = vec!["box.step", "body.stl", "notes.md"];
    if spec.lid {
        required.push("lid.stl");
    }

    for name in &required {
        let path = out_dir.join(name);
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() > 0 => {}
            Ok(_) => {
                return Err(build_error(
                    &spec.id,
                    format!("cannot publish: `{}` is empty", path.display()),
                ));
            }
            Err(error) => {
                return Err(build_error(
                    &spec.id,
                    format!("cannot publish: `{}` is missing: {error}", path.display()),
                ));
            }
        }
    }

    for name in required {
        let source = out_dir.join(name);
        let destination = target.join(name);
        if name == "notes.md" && destination.exists() {
            // The human print record wins; the generated placeholder is only
            // published for a box folder that does not have one yet.
            tracing::debug!(
                path = %destination.display(),
                "keeping the existing notes.md"
            );
            continue;
        }
        fs::copy(&source, &destination).map_err(|error| BoxgenError::Io {
            message: format!(
                "cannot publish `{}` to `{}`: {error}",
                source.display(),
                destination.display()
            ),
        })?;
    }

    Ok(())
}

/// Renders the initial `notes.md` for a build that does not have one.
///
/// The structure follows the standard's box record: the spec notes and
/// contents, the computed stack group, and an empty print record for the human
/// to fill in. Later `index` runs read the print date from these lines.
pub(crate) fn render_notes(spec: &Spec) -> String {
    use std::fmt::Write as _;

    let body = spec.body();
    let mut out = String::new();
    let _ = writeln!(out, "# {}", spec.id);
    let _ = writeln!(out);

    if let Some(notes) = spec
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|notes| !notes.is_empty())
    {
        let _ = writeln!(out, "{notes}");
        let _ = writeln!(out);
    }
    if let Some(description) = spec
        .contents
        .as_ref()
        .and_then(|contents| contents.description.as_deref())
        .map(str::trim)
        .filter(|description| !description.is_empty())
    {
        let _ = writeln!(out, "Contents: {description}");
    }

    let _ = writeln!(
        out,
        "Footprint: {} × {} ({} × {} mm)",
        spec.footprint[0],
        spec.footprint[1],
        fmt_mm(body.outer_width_mm),
        fmt_mm(body.outer_depth_mm)
    );
    let _ = writeln!(out, "Height: {}U", spec.height_u);
    let _ = writeln!(
        out,
        "Stack group: {}",
        crate::index::stack_group(spec.footprint)
    );
    let _ = writeln!(out, "Lid: {}", if spec.lid { "yes" } else { "no" });
    let _ = writeln!(out);
    let _ = writeln!(out, "## Print record");
    let _ = writeln!(out);
    let _ = writeln!(out, "Printed:");
    let _ = writeln!(out, "Filament spool/lot:");
    let _ = writeln!(out, "Nozzle:");
    let _ = writeln!(out, "Coupon result:");
    let _ = writeln!(out, "Actual mass:");
    let _ = writeln!(out, "Actual print time:");

    out
}

/// Renders the generated `box.py` for one validated spec.
///
/// The script is a pure function of the spec and this master: no timestamps,
/// no temporary paths, and a stable JSON field order. `freecadcmd` does not set
/// `__name__ == "__main__"` and exits 0 on an unhandled exception, so the entry
/// point runs at module level and is wrapped in `try/except` + `sys.exit(1)`.
pub fn render_box_py(spec: &Spec) -> String {
    let spec_json = serde_json::to_string_pretty(&spec_json(spec))
        .expect("a parsed spec always serializes to JSON");
    let embedded = if spec_json.contains("'''") {
        // A raw triple-quoted string cannot contain its own delimiter. The JSON
        // encoding of the JSON text is also a valid Python string literal.
        serde_json::to_string(&spec_json).expect("a JSON string always serializes")
    } else {
        format!("r'''{spec_json}'''")
    };

    format!(
        "# Generated by boxgen - do not edit.\n\
         \"\"\"FreeCAD build script for boxgen id {id}.\n\
         \n\
         Run with: freecadcmd box.py\n\
         The script imports the cad/ master next to it and writes report.json.\n\
         \"\"\"\n\
         import json\n\
         import os\n\
         import sys\n\
         import traceback\n\
         \n\
         HERE = os.path.dirname(os.path.abspath(__file__))\n\
         sys.path.insert(0, HERE)\n\
         \n\
         SPEC = json.loads({embedded})\n\
         \n\
         \n\
         def main():\n\
         \x20   import cad.build as master\n\
         \n\
         \x20   report = master.build(SPEC, HERE)\n\
         \x20   with open(os.path.join(HERE, \"report.json\"), \"w\", encoding=\"utf-8\", newline=\"\\n\") as handle:\n\
         \x20       json.dump(report, handle, indent=2, sort_keys=True)\n\
         \x20   return 0\n\
         \n\
         \n\
         # freecadcmd does not set __name__ == \"__main__\"; run the entry point at module level.\n\
         try:\n\
         \x20   sys.exit(main())\n\
         except SystemExit:\n\
         \x20   raise\n\
         except BaseException:\n\
         \x20   traceback.print_exc()\n\
         \x20   sys.exit(1)\n",
        id = spec.id,
        embedded = embedded,
    )
}

/// Serializes a parsed spec in the closed schema shape for the Python master.
fn spec_json(spec: &Spec) -> Value {
    let mut root = serde_json::Map::new();
    root.insert("system".to_owned(), json!(spec.system));
    root.insert("id".to_owned(), json!(spec.id));
    root.insert("footprint".to_owned(), json!(spec.footprint));
    root.insert("height_u".to_owned(), json!(spec.height_u));
    root.insert("lid".to_owned(), json!(spec.lid));
    root.insert("lid_ribs".to_owned(), json!(spec.lid_ribs));
    if spec.coupon {
        // Internal flag; only the coupon path sets it. Emitting it only when
        // true keeps every user-spec golden script byte-identical.
        root.insert("coupon".to_owned(), json!(true));
    }
    if let Some(notes) = &spec.notes {
        root.insert("notes".to_owned(), json!(notes));
    }
    if let Some(contents) = &spec.contents {
        root.insert("contents".to_owned(), contents_json(contents));
    }
    root.insert("divisions".to_owned(), node_json(&spec.divisions));
    Value::Object(root)
}

fn contents_json(contents: &Contents) -> Value {
    let mut map = serde_json::Map::new();
    if let Some(description) = &contents.description {
        map.insert("description".to_owned(), json!(description));
    }
    if contents.soft {
        // Emitted only when true, so specs that do not set the rigid default
        // keep their generated scripts byte-identical.
        map.insert("soft".to_owned(), json!(true));
    }
    if let Some(value) = contents.max_length_mm {
        map.insert("max_length_mm".to_owned(), json!(value));
    }
    if let Some(value) = contents.max_width_mm {
        map.insert("max_width_mm".to_owned(), json!(value));
    }
    if let Some(value) = contents.max_height_mm {
        map.insert("max_height_mm".to_owned(), json!(value));
    }
    if let Some(value) = contents.weight_kg {
        map.insert("weight_kg".to_owned(), json!(value));
    }
    Value::Object(map)
}

fn node_json(node: &Node) -> Value {
    match node {
        Node::Leaf { compartment } => {
            let mut map = serde_json::Map::new();
            if let Some(name) = compartment {
                map.insert("compartment".to_owned(), json!(name));
            }
            Value::Object(map)
        }
        Node::Split {
            axis,
            at_mm,
            near,
            far,
        } => {
            let mut map = serde_json::Map::new();
            map.insert("axis".to_owned(), json!(axis.as_str()));
            map.insert("at_mm".to_owned(), json!(at_mm));
            map.insert("near".to_owned(), node_json(near));
            map.insert("far".to_owned(), node_json(far));
            Value::Object(map)
        }
    }
}

/// Resolves the checked-in FreeCAD master directory.
///
/// `boxgen` is a local CLI built from this repository; the master ships beside
/// the crate and is copied into every build directory so `box.py` is portable
/// and reviewable.
fn repo_cad_dir() -> Result<PathBuf, BoxgenError> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cad");
    if !dir.is_dir() {
        return Err(BoxgenError::Environment {
            message: format!(
                "the FreeCAD master directory `{}` is missing; build boxgen from its repository",
                dir.display()
            ),
        });
    }
    Ok(dir)
}

/// Creates the output directory and removes only the artifacts boxgen owns.
fn prepare_out_dir(out_dir: &Path) -> Result<(), BoxgenError> {
    fs::create_dir_all(out_dir).map_err(|error| BoxgenError::Io {
        message: format!(
            "cannot create output directory `{}`: {error}",
            out_dir.display()
        ),
    })?;

    let cad_source = repo_cad_dir()?;
    let output_cad = fs::canonicalize(out_dir)
        .map(|path| path.join("cad"))
        .unwrap_or_else(|_| out_dir.join("cad"));
    let source_cad = fs::canonicalize(&cad_source).unwrap_or(cad_source);
    if output_cad == source_cad {
        return Err(BoxgenError::Usage {
            message: format!(
                "output directory `{}` contains the repository's cad/ master; choose another --out",
                out_dir.display()
            ),
        });
    }

    for name in GENERATED_ARTIFACTS {
        remove_file_if_exists(&out_dir.join(name))?;
    }

    let cad_dir = out_dir.join("cad");
    if cad_dir.exists() {
        fs::remove_dir_all(&cad_dir).map_err(|error| BoxgenError::Io {
            message: format!("cannot remove stale `{}`: {error}", cad_dir.display()),
        })?;
    }

    let pycache = out_dir.join("__pycache__");
    if pycache.exists() {
        fs::remove_dir_all(&pycache).map_err(|error| BoxgenError::Io {
            message: format!("cannot remove stale `{}`: {error}", pycache.display()),
        })?;
    }

    remove_backups(out_dir);
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<(), BoxgenError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(BoxgenError::Io {
            message: format!("cannot remove stale `{}`: {error}", path.display()),
        }),
    }
}

/// Copies only the importable master modules and the LGPL license text.
///
/// `cad/tests/`, `__pycache__/`, and `*.pyc` never enter a build directory.
fn copy_cad(source: &Path, destination: &Path) -> Result<(), BoxgenError> {
    fs::create_dir_all(destination).map_err(|error| BoxgenError::Io {
        message: format!("cannot create `{}`: {error}", destination.display()),
    })?;

    let mut entries = fs::read_dir(source)
        .map_err(|error| BoxgenError::Io {
            message: format!(
                "cannot read the FreeCAD master `{}`: {error}",
                source.display()
            ),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| BoxgenError::Io {
            message: format!(
                "cannot read the FreeCAD master `{}`: {error}",
                source.display()
            ),
        })?;
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let file_type = entry.file_type().map_err(|error| BoxgenError::Io {
            message: format!("cannot inspect `{}`: {error}", entry.path().display()),
        })?;
        if !file_type.is_file() {
            continue;
        }

        let name = entry.file_name();
        let keep = name.to_str().is_some_and(|name| {
            (name.ends_with(".py") && !name.ends_with(".pyc")) || name == CAD_LICENSE
        });
        if !keep {
            continue;
        }

        let target = destination.join(&name);
        fs::copy(entry.path(), &target).map_err(|error| BoxgenError::Io {
            message: format!(
                "cannot copy `{}` to `{}`: {error}",
                entry.path().display(),
                target.display()
            ),
        })?;
    }

    Ok(())
}

/// Verifies the FreeCAD launcher exists and reports the pinned release.
fn check_freecad(config: &Config) -> Result<(), BoxgenError> {
    let output = Command::new(&config.freecadcmd)
        .arg("--version")
        .output()
        .map_err(|error| BoxgenError::Environment {
            message: format!(
                "cannot run `{} --version`: {error}; set {ENV_FREECADCMD} or install FreeCAD {}",
                config.freecadcmd.display(),
                config.expected_freecad_version
            ),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let reported = format!("{stdout}{stderr}");
    let reported = reported.trim();

    if !output.status.success() {
        return Err(BoxgenError::Environment {
            message: format!(
                "`{} --version` exited with {}: {reported}",
                config.freecadcmd.display(),
                output.status
            ),
        });
    }

    if !reported.contains(&config.expected_freecad_version) {
        return Err(BoxgenError::Environment {
            message: format!(
                "`{} --version` does not report the expected FreeCAD {} (set {ENV_FREECAD_VERSION}); reported: {reported}",
                config.freecadcmd.display(),
                config.expected_freecad_version
            ),
        });
    }

    tracing::debug!(
        launcher = %config.freecadcmd.display(),
        version = %config.expected_freecad_version,
        "FreeCAD environment check passed"
    );
    Ok(())
}

/// Checks that FreeCAD produced every artifact and that `report.json` parses.
fn read_report(out_dir: &Path, spec: &Spec) -> Result<Value, BoxgenError> {
    for name in required_outputs(spec.lid) {
        let path = out_dir.join(name);
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() > 0 => {}
            Ok(_) => {
                return Err(build_error(
                    &spec.id,
                    format!("FreeCAD wrote an empty `{}`", path.display()),
                ));
            }
            Err(error) => {
                return Err(build_error(
                    &spec.id,
                    format!(
                        "FreeCAD did not write `{}`: {error}; freecadcmd exits 0 on an unhandled exception, so the script output was not trusted",
                        path.display()
                    ),
                ));
            }
        }
    }

    let path = out_dir.join("report.json");
    let text = fs::read_to_string(&path).map_err(|error| {
        build_error(
            &spec.id,
            format!("cannot read `{}`: {error}", path.display()),
        )
    })?;
    serde_json::from_str(&text).map_err(|error| {
        build_error(
            &spec.id,
            format!("`{}` is not valid JSON: {error}", path.display()),
        )
    })
}

/// Adds the Rust validation warnings to the Python report without duplicating.
fn merge_warnings(
    report: &mut Value,
    validation_warnings: &[String],
    id: &str,
) -> Result<(), BoxgenError> {
    let object = report
        .as_object_mut()
        .ok_or_else(|| build_error(id, "report.json must contain one JSON object".to_owned()))?;

    let mut warnings = validation_warnings.to_vec();
    if let Some(existing) = object.get("warnings") {
        let existing = existing
            .as_array()
            .ok_or_else(|| build_error(id, "report.json `warnings` must be a list".to_owned()))?;
        for warning in existing {
            let warning = warning.as_str().ok_or_else(|| {
                build_error(
                    id,
                    "report.json `warnings` entries must be strings".to_owned(),
                )
            })?;
            if !warnings.iter().any(|known| known == warning) {
                warnings.push(warning.to_owned());
            }
        }
    }

    object.insert("warnings".to_owned(), json!(warnings));
    Ok(())
}

/// Enforces the contract's post-build invariants on the report.
fn validate_report(report: &BuildReport, spec: &Spec) -> Result<(), BoxgenError> {
    let id = spec.id.as_str();
    if report.id != id {
        return Err(build_error(
            id,
            format!(
                "report.json id {:?} does not match the spec id {id:?}",
                report.id
            ),
        ));
    }
    require_solid_report(
        id,
        "body",
        report.body.solids,
        report.body.volume_mm3,
        report.body.is_valid,
        report.body.is_closed,
    )?;
    let fillet = report.body.floor_fillet_mm;
    if (fillet - 2.0).abs() > 1e-6 && (fillet - 1.0).abs() > 1e-6 {
        return Err(build_error(
            id,
            format!(
                "the floor fillet must be 2.0 mm or the 1.0 mm fallback, report.json says {fillet} mm"
            ),
        ));
    }
    if !report.stl.is_solid {
        return Err(build_error(
            id,
            "body.stl is not a watertight solid mesh".to_owned(),
        ));
    }

    match (&report.coupon, spec.coupon) {
        (Some(coupon), true) => {
            if !coupon.t_divider {
                return Err(build_error(
                    id,
                    "report.json must record the coupon's T-divider".to_owned(),
                ));
            }
            if coupon.wall.is_empty() {
                return Err(build_error(
                    id,
                    "the coupon report must name the wall its T-divider is fused to".to_owned(),
                ));
            }
            if (coupon.thickness_mm - tree::DIVIDER_MM).abs() > 1e-6 {
                return Err(build_error(
                    id,
                    format!(
                        "the coupon T-divider must be {:.1} mm thick, report.json says {} mm",
                        tree::DIVIDER_MM,
                        coupon.thickness_mm
                    ),
                ));
            }
            if coupon.length_mm < 20.0 - 1e-6 {
                return Err(build_error(
                    id,
                    format!(
                        "the coupon T-divider must cover the calibration's 20 mm foot, report.json says {} mm",
                        coupon.length_mm
                    ),
                ));
            }
            let body = spec.body();
            if (coupon.height_above_floor_mm - body.divider_height_mm).abs() > 1e-6 {
                return Err(build_error(
                    id,
                    format!(
                        "the coupon T-divider height must be {:.2} mm above the floor, report.json says {} mm",
                        body.divider_height_mm, coupon.height_above_floor_mm
                    ),
                ));
            }
            let expected_top = tree::BASE_HEIGHT_MM + body.divider_height_mm;
            if (coupon.top_mm - expected_top).abs() > 1e-6 {
                return Err(build_error(
                    id,
                    format!(
                        "the coupon T-divider top must be {expected_top:.2} mm, report.json says {} mm",
                        coupon.top_mm
                    ),
                ));
            }
            if (coupon.top_chamfer_mm - tree::DIVIDER_TOP_CHAMFER_MM).abs() > 1e-6 {
                return Err(build_error(
                    id,
                    format!(
                        "the coupon T-divider top chamfer must be {:.1} mm, report.json says {} mm",
                        tree::DIVIDER_TOP_CHAMFER_MM,
                        coupon.top_chamfer_mm
                    ),
                ));
            }
        }
        (None, true) => {
            return Err(build_error(
                id,
                "report.json has no coupon section for the coupon build".to_owned(),
            ));
        }
        (Some(_), false) => {
            return Err(build_error(
                id,
                "report.json has a coupon section but the spec is not a coupon".to_owned(),
            ));
        }
        (None, false) => {}
    }

    match (&report.lid, spec.lid) {
        (Some(lid), true) => {
            require_solid_report(
                id,
                "lid",
                lid.solids,
                lid.volume_mm3,
                lid.is_valid,
                lid.is_closed,
            )?;
            if lid.ribs != spec.lid_ribs {
                return Err(build_error(
                    id,
                    format!(
                        "the lid report must record lid_ribs={}, report.json says {}",
                        spec.lid_ribs, lid.ribs
                    ),
                ));
            }
            if !report.lid_stl.as_ref().is_some_and(|stl| stl.is_solid) {
                return Err(build_error(
                    id,
                    "lid.stl is not a watertight solid mesh".to_owned(),
                ));
            }
        }
        (None, true) => {
            return Err(build_error(
                id,
                "report.json has no lid section for a lid build".to_owned(),
            ));
        }
        (Some(_), false) => {
            return Err(build_error(
                id,
                "report.json has a lid section but the spec sets lid: false".to_owned(),
            ));
        }
        (None, false) => {}
    }

    Ok(())
}

fn build_error(id: &str, message: String) -> BoxgenError {
    BoxgenError::Build {
        id: id.to_owned(),
        message,
    }
}

/// Enforces the one-solid, positive-volume, valid, closed-shell invariants on
/// one report part (the body or the lid).
fn require_solid_report(
    id: &str,
    part: &str,
    solids: u64,
    volume_mm3: f64,
    is_valid: bool,
    is_closed: bool,
) -> Result<(), BoxgenError> {
    if solids != 1 {
        return Err(build_error(
            id,
            format!("the {part} must be exactly one solid, report.json says {solids}"),
        ));
    }
    if !volume_mm3.is_finite() || volume_mm3 <= 0.0 {
        return Err(build_error(
            id,
            format!(
                "the {part} volume must be positive and finite, report.json says {volume_mm3} mm3"
            ),
        ));
    }
    if !is_valid {
        return Err(build_error(id, format!("the {part} shape is not valid")));
    }
    if !is_closed {
        return Err(build_error(
            id,
            format!("the {part} must be one closed shell (watertight)"),
        ));
    }
    Ok(())
}

fn write_file(path: &Path, text: &str) -> Result<(), BoxgenError> {
    fs::write(path, text).map_err(|error| BoxgenError::Io {
        message: format!("cannot write `{}`: {error}", path.display()),
    })
}

fn write_report(path: &Path, report: &Value) -> Result<(), BoxgenError> {
    let text = serde_json::to_string_pretty(report).expect("a JSON value always serializes");
    write_file(path, &format!("{text}\n"))
}

/// `doc.saveAs` drops a timestamped backup beside `box.FCStd`; it is not an
/// artifact and would make the output directory depend on wall-clock time.
fn remove_backups(out_dir: &Path) {
    let Ok(entries) = fs::read_dir(out_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with("box.") && name.ends_with(".FCBak") {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Removes the `__pycache__` directories FreeCAD's interpreter creates.
fn remove_runtime_caches(out_dir: &Path) {
    for path in [
        out_dir.join("__pycache__"),
        out_dir.join("cad").join("__pycache__"),
    ] {
        if path.exists() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn diagnostic_tail(stderr: &[u8], stdout: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stdout = String::from_utf8_lossy(stdout);
    let source = if stderr.trim().is_empty() {
        stdout.as_ref()
    } else {
        stderr.as_ref()
    };
    let lines = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return "(no diagnostics)".to_owned();
    }
    lines[lines.len().saturating_sub(20)..].join("\n")
}

fn print_summary(out_dir: &Path, report: &BuildReport) {
    println!("id: {}", report.id);
    println!("output: {}", out_dir.display());
    println!(
        "outer: {:.1} x {:.1} x {:.1} mm",
        report.dimensions.outer_x_mm, report.dimensions.outer_y_mm, report.dimensions.height_mm
    );
    println!("interior depth: {:.2} mm", report.interior_depth_mm);
    println!(
        "body: {:.1} mm3, {} faces, {:.1} mm2 surface",
        report.body.volume_mm3, report.body.faces, report.body.area_mm2
    );
    if let Some(lid) = &report.lid {
        println!(
            "lid: {:.1} mm3, {} faces, {:.1} mm2 surface, ribs: {}, roundover: {:.1} mm",
            lid.volume_mm3,
            lid.faces,
            lid.area_mm2,
            if lid.ribs { "yes" } else { "no" },
            lid.roundover_mm
        );
    }
    println!(
        "floor fillet: {:.1} mm (standard 2.0, fallback 1.0)",
        report.body.floor_fillet_mm
    );
    println!(
        "stl: {} facets, {:.1} mm3, watertight",
        report.stl.facets, report.stl.volume_mm3
    );
    let mass_scope = if report.lid.is_some() {
        "body + lid"
    } else {
        "body only"
    };
    println!(
        "mass: {:.1} g ({mass_scope}; PETG {:.2} g/cm3)",
        report.mass_g,
        crate::tree::PETG_DENSITY_G_PER_CM3
    );
    println!(
        "print time: {:.2} h (at {:.1} mm3/s effective flow)",
        report.print_time_h,
        crate::tree::EFFECTIVE_FLOW_MM3_PER_S
    );
    if report.warnings.is_empty() {
        println!("warnings: none");
    } else {
        println!("warnings:");
        for warning in &report.warnings {
            println!("  - {warning}");
        }
    }
    let artifacts = match report.artifacts.lid_stl_bytes {
        Some(lid_bytes) => format!(
            "box.step ({} bytes), body.stl ({} bytes), lid.stl ({lid_bytes} bytes)",
            report.artifacts.box_step_bytes, report.artifacts.body_stl_bytes
        ),
        None => format!(
            "box.step ({} bytes), body.stl ({} bytes)",
            report.artifacts.box_step_bytes, report.artifacts.body_stl_bytes
        ),
    };
    println!("artifacts: {artifacts}");
}

#[derive(Debug, Deserialize)]
struct BuildReport {
    id: String,
    dimensions: Dimensions,
    interior_depth_mm: f64,
    body: BodyReport,
    /// Lid measurements; present exactly when the spec has `lid: true`.
    #[serde(default)]
    lid: Option<LidReport>,
    /// Calibration T-divider facts; present exactly when the internal
    /// `coupon` flag is set (ticket #10).
    #[serde(default)]
    coupon: Option<CouponReport>,
    stl: StlReport,
    /// `lid.stl` mesh facts; present exactly when the spec has `lid: true`.
    #[serde(default)]
    lid_stl: Option<StlReport>,
    artifacts: ArtifactSizes,
    mass_g: f64,
    print_time_h: f64,
    #[serde(default)]
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Dimensions {
    outer_x_mm: f64,
    outer_y_mm: f64,
    height_mm: f64,
}

#[derive(Debug, Deserialize)]
struct BodyReport {
    volume_mm3: f64,
    area_mm2: f64,
    faces: u64,
    solids: u64,
    is_valid: bool,
    is_closed: bool,
    /// Floor-to-wall fillet radius actually applied: 2.0 mm, or the 1.0 mm
    /// fallback (standard §4.1). Never anything else: a sharp internal corner
    /// fails the build instead.
    floor_fillet_mm: f64,
}

/// Lid measurements (ticket #9, standard §4.4).
#[derive(Debug, Deserialize)]
struct LidReport {
    volume_mm3: f64,
    area_mm2: f64,
    faces: u64,
    solids: u64,
    is_valid: bool,
    is_closed: bool,
    /// Top-edge roundover radius actually applied: 1.0 mm, or 0.0 when the
    /// 1.0 mm roundover on the 2.0 mm plate was rejected. Recorded like
    /// `body.floor_fillet_mm`, with a warning on 0.0.
    roundover_mm: f64,
    /// Whether the optional 0.15 mm friction ribs are part of the lid.
    ribs: bool,
}

/// Calibration T-divider facts (ticket #10, calibration §2).
///
/// Present exactly when the internal `coupon` flag is set, so the master
/// cannot silently skip the coupon divider and still pass the build.
#[derive(Debug, Deserialize)]
struct CouponReport {
    /// Whether the coupon's T-divider was fused into the body.
    t_divider: bool,
    /// The wall it is fused to (documented in `cad/dividers.py`).
    wall: String,
    /// Plate length from the wall's inner face, in millimetres. Must cover
    /// the calibration's 20 mm weight foot.
    length_mm: f64,
    /// Plate thickness, the standard 2.0 mm divider.
    thickness_mm: f64,
    /// Height above the floor: `interior_depth - 10.0` on the 6U coupon.
    height_above_floor_mm: f64,
    /// Absolute top elevation: base height plus the height above the floor
    /// (28.0 mm on the 6U coupon, 10 mm below the lip bottom).
    top_mm: f64,
    /// Top-edge chamfer, the standard 0.4 mm divider edge break.
    top_chamfer_mm: f64,
}

#[derive(Debug, Deserialize)]
struct StlReport {
    facets: u64,
    is_solid: bool,
    volume_mm3: f64,
}

#[derive(Debug, Deserialize)]
struct ArtifactSizes {
    box_step_bytes: u64,
    body_stl_bytes: u64,
    /// Present exactly when the spec has `lid: true`.
    #[serde(default)]
    lid_stl_bytes: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = include_str!("../tests/fixtures/minimal-build.yaml");

    fn minimal_spec() -> Spec {
        spec::parse(MINIMAL).expect("the golden fixture parses")
    }

    #[test]
    fn generated_script_embeds_the_spec_and_a_safe_entry_point() {
        let script = render_box_py(&minimal_spec());

        assert!(script.starts_with("# Generated by boxgen - do not edit.\n"));
        assert!(script.contains("SPEC = json.loads(r'''{"));
        assert!(script.contains("\"id\": \"1x1-6U-minimal-v1\""));
        assert!(script.contains("\"compartment\": \"Minimal\""));
        assert!(script.contains("sys.path.insert(0, HERE)"));
        assert!(script.contains("import cad.build as master"));
        assert!(script.contains("except BaseException:"));
        assert!(script.contains("sys.exit(1)"));
        assert!(!script.contains("if __name__ =="));
    }

    #[test]
    fn generated_script_is_deterministic() {
        let spec = minimal_spec();
        assert_eq!(render_box_py(&spec), render_box_py(&spec));
    }

    #[test]
    fn a_compartment_with_a_triple_quote_falls_back_to_a_quoted_literal() {
        let spec = spec::parse(
            "system: SYS-1.0\nid: 1x1-6U-quote-v1\nfootprint: [1, 1]\nheight_u: 6\nlid: false\ndivisions: { compartment: \"a'''b\" }\n",
        )
        .expect("the fixture parses");

        let script = render_box_py(&spec);

        assert!(script.contains("SPEC = json.loads(\"{\\n"), "{script}");
        assert!(!script.contains("r'''"), "{script}");
    }

    #[test]
    fn diagnostic_tail_prefers_stderr_and_keeps_the_last_lines() {
        let stderr = (0..30)
            .map(|index| format!("error {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let tail = diagnostic_tail(stderr.as_bytes(), b"stdout noise");

        assert_eq!(tail.lines().count(), 20);
        assert!(tail.contains("error 29"));
        assert!(!tail.contains("error 9\n"));
        assert_eq!(diagnostic_tail(b"", b"stdout only"), "stdout only");
        assert_eq!(diagnostic_tail(b"", b""), "(no diagnostics)");
    }

    fn lid_spec() -> Spec {
        spec::parse(
            "system: SYS-1.0\n\
             id: 4x4-6U-lid-v1\n\
             footprint: [4, 4]\n\
             height_u: 6\n\
             lid: true\n\
             divisions: { compartment: Lid }\n",
        )
        .expect("the lid fixture parses")
    }

    fn lid_ribs_spec() -> Spec {
        spec::parse(
            "system: SYS-1.0\n\
             id: 4x4-6U-lid-v1\n\
             footprint: [4, 4]\n\
             height_u: 6\n\
             lid: true\n\
             lid_ribs: true\n\
             divisions: { compartment: Lid }\n",
        )
        .expect("the ribbon lid fixture parses")
    }

    #[test]
    fn generated_script_embeds_lid_ribs() {
        let script = render_box_py(&lid_ribs_spec());

        assert!(script.contains("\"lid\": true"), "{script}");
        assert!(script.contains("\"lid_ribs\": true"), "{script}");

        let plain = render_box_py(&lid_spec());
        assert!(plain.contains("\"lid_ribs\": false"), "{plain}");
    }

    /// `contents.soft` reaches the embedded spec only when true, so specs
    /// that keep the rigid default stay byte-identical to earlier scripts.
    #[test]
    fn generated_script_embeds_soft_contents_only_when_true() {
        let rigid = spec::parse(
            "system: SYS-1.0\nid: 1x1-6U-soft-v1\nfootprint: [1, 1]\nheight_u: 6\nlid: false\ncontents:\n  max_width_mm: 30\ndivisions: { compartment: Soft }\n",
        )
        .expect("the rigid contents fixture parses");
        let script = render_box_py(&rigid);
        assert!(script.contains("\"contents\""), "{script}");
        assert!(!script.contains("\"soft\""), "{script}");

        let soft = spec::parse(
            "system: SYS-1.0\nid: 1x1-6U-soft-v1\nfootprint: [1, 1]\nheight_u: 6\nlid: false\ncontents:\n  soft: true\n  max_width_mm: 30\ndivisions: { compartment: Soft }\n",
        )
        .expect("the soft contents fixture parses");
        assert!(
            render_box_py(&soft).contains("\"soft\": true"),
            "soft contents must be embedded: {}",
            render_box_py(&soft)
        );
    }

    /// The coupon spec is fixed by calibration section 2: 1 x 1, 6U, the
    /// ribbed lid, one leaf, and the internal flag. The id is internal and
    /// deliberately outside the user-facing id rule.
    #[test]
    fn coupon_spec_is_the_calibration_coupon() {
        let spec = coupon_spec();

        assert_eq!(spec.system, spec::SYSTEM_VERSION);
        assert_eq!(spec.id, "coupon-1x1-6U-v1");
        assert_eq!(spec.footprint, [1, 1]);
        assert_eq!(spec.height_u, 6);
        assert!(spec.lid, "the coupon has the friction lid");
        assert!(spec.lid_ribs, "the coupon lid carries the 0.15 mm ribs");
        assert!(spec.coupon, "the internal coupon flag drives the T-divider");
        assert_eq!(spec.divisions, Node::leaf(Some("Coupon")));

        // The coupon body is the standard 1 x 1 x 6U geometry: 41.5 mm outer,
        // 42.0 mm tall, divider top at base + 23.25 = 28.0 mm.
        let body = spec.body();
        assert!((body.outer_width_mm - 41.5).abs() < 1e-9);
        assert!((body.total_height_mm - 42.0).abs() < 1e-9);
        assert!(
            (tree::BASE_HEIGHT_MM + body.divider_height_mm - 28.0).abs() < 1e-9,
            "the 6U coupon divider top must be 28.0 mm"
        );
    }

    /// The generated coupon script embeds the internal flag and the same
    /// `cad.build` entry point as every other build.
    #[test]
    fn the_coupon_script_embeds_the_internal_flag() {
        let script = render_box_py(&coupon_spec());

        assert!(script.contains("\"coupon\": true"), "{script}");
        assert!(script.contains("\"id\": \"coupon-1x1-6U-v1\""), "{script}");
        assert!(script.contains("\"lid_ribs\": true"), "{script}");
        assert!(script.contains("import cad.build as master"), "{script}");
        assert!(
            script.contains("report = master.build(SPEC, HERE)"),
            "{script}"
        );
    }

    /// `lid.stl` is required exactly for a lid spec and never for a body-only
    /// one; boxgen also owns (and clears) it on every build.
    #[test]
    fn required_outputs_require_lid_stl_only_for_a_lid_spec() {
        let body = required_outputs(false);
        assert!(body.contains(&"box.step"));
        assert!(body.contains(&"body.stl"));
        assert!(!body.contains(&"lid.stl"));

        let lid = required_outputs(true);
        assert!(lid.contains(&"lid.stl"));
        assert!(lid.contains(&"body.stl"));
        assert!(GENERATED_ARTIFACTS.contains(&"lid.stl"));
    }

    /// A complete fake report, with or without the lid sections.
    fn report_value(id: &str, with_lid: bool) -> Value {
        let mut value = json!({
            "id": id,
            "dimensions": {"outer_x_mm": 167.5, "outer_y_mm": 167.5, "height_mm": 42.0},
            "interior_depth_mm": 33.25,
            "body": {
                "volume_mm3": 1.0,
                "area_mm2": 1.0,
                "faces": 1,
                "solids": 1,
                "is_valid": true,
                "is_closed": true,
                "floor_fillet_mm": 2.0,
            },
            "stl": {"facets": 1, "is_solid": true, "volume_mm3": 1.0},
            "artifacts": {"box_step_bytes": 1, "body_stl_bytes": 1},
            "mass_g": 1.0,
            "print_time_h": 0.1,
            "warnings": [],
        });
        if with_lid {
            value["lid"] = json!({
                "volume_mm3": 1.0,
                "area_mm2": 1.0,
                "faces": 1,
                "solids": 1,
                "is_valid": true,
                "is_closed": true,
                "roundover_mm": 1.0,
                "ribs": false,
                "bbox": {
                    "x_min": -1.0, "x_max": 1.0,
                    "y_min": -1.0, "y_max": 1.0,
                    "z_min": 37.0, "z_max": 41.0,
                },
            });
            value["lid_stl"] = json!({"facets": 1, "is_solid": true, "volume_mm3": 1.0});
            value["artifacts"]["lid_stl_bytes"] = json!(1);
        }
        value
    }

    fn parsed_report(value: &Value) -> BuildReport {
        serde_json::from_value(value.clone()).expect("the report fixture matches the schema")
    }

    #[test]
    fn validate_report_accepts_a_complete_lid_report() {
        validate_report(
            &parsed_report(&report_value("4x4-6U-lid-v1", true)),
            &lid_spec(),
        )
        .expect("a complete lid report must pass");
        validate_report(
            &parsed_report(&report_value("1x1-6U-minimal-v1", false)),
            &minimal_spec(),
        )
        .expect("a complete body-only report must pass");
    }

    #[test]
    fn validate_report_requires_the_lid_section_for_a_lid_spec() {
        let error = validate_report(
            &parsed_report(&report_value("4x4-6U-lid-v1", false)),
            &lid_spec(),
        )
        .expect_err("a lid spec without a lid section must fail");
        assert!(error.to_string().contains("lid"), "{error}");
    }

    #[test]
    fn validate_report_rejects_a_lid_section_without_a_lid() {
        let error = validate_report(
            &parsed_report(&report_value("1x1-6U-minimal-v1", true)),
            &minimal_spec(),
        )
        .expect_err("a body-only spec with a lid section must fail");
        assert!(error.to_string().contains("lid"), "{error}");
    }

    #[test]
    fn validate_report_rejects_a_lid_stl_that_is_not_a_solid() {
        let mut value = report_value("4x4-6U-lid-v1", true);
        value["lid_stl"]["is_solid"] = json!(false);
        let error = validate_report(&parsed_report(&value), &lid_spec())
            .expect_err("a non-solid lid mesh must fail");
        assert!(error.to_string().contains("lid.stl"), "{error}");
    }

    #[test]
    fn validate_report_rejects_a_rib_mismatch() {
        let mut value = report_value("4x4-6U-lid-v1", true);
        value["lid"]["ribs"] = json!(true);
        let error = validate_report(&parsed_report(&value), &lid_spec())
            .expect_err("a rib mismatch must fail");
        assert!(error.to_string().contains("ribs"), "{error}");

        validate_report(&parsed_report(&value), &lid_ribs_spec())
            .expect("the ribbed spec must accept a ribbed report");
    }

    /// A complete coupon report: the lid sections of a lid spec plus the
    /// calibration T-divider facts.
    fn coupon_report_value() -> Value {
        let mut value = report_value("coupon-1x1-6U-v1", true);
        value["lid"]["ribs"] = json!(true);
        value["coupon"] = json!({
            "t_divider": true,
            "wall": "-y",
            "length_mm": 20.0,
            "thickness_mm": 2.0,
            "height_above_floor_mm": 23.25,
            "top_mm": 28.0,
            "top_chamfer_mm": 0.4,
        });
        value
    }

    /// The coupon spec requires the T-divider section, and no other spec may
    /// carry one: the master cannot silently skip the coupon geometry.
    #[test]
    fn validate_report_requires_the_coupon_section_only_for_the_coupon() {
        validate_report(&parsed_report(&coupon_report_value()), &coupon_spec())
            .expect("a complete coupon report must pass");

        let error = validate_report(
            &parsed_report(&report_value("coupon-1x1-6U-v1", true)),
            &coupon_spec(),
        )
        .expect_err("a coupon without the T-divider section must fail");
        assert!(error.to_string().contains("coupon section"), "{error}");

        let mut value = report_value("1x1-6U-minimal-v1", false);
        value["coupon"] = coupon_report_value()["coupon"].clone();
        let error = validate_report(&parsed_report(&value), &minimal_spec())
            .expect_err("a non-coupon spec with a coupon section must fail");
        assert!(error.to_string().contains("not a coupon"), "{error}");
    }

    /// The coupon section pins the calibration numbers: 2.0 mm thick, the 6U
    /// height above the floor, the 28.0 mm top, and at least the 20 mm foot.
    #[test]
    fn validate_report_pins_the_coupon_divider_numbers() {
        for (field, wrong) in [
            ("thickness_mm", 1.0),
            ("height_above_floor_mm", 22.0),
            ("top_mm", 27.0),
            ("length_mm", 19.0),
            ("top_chamfer_mm", 0.3),
        ] {
            let mut value = coupon_report_value();
            value["coupon"][field] = json!(wrong);
            let result = validate_report(&parsed_report(&value), &coupon_spec());
            assert!(result.is_err(), "a wrong {field} ({wrong}) must fail");
        }

        let mut value = coupon_report_value();
        value["coupon"]["t_divider"] = json!(false);
        let error = validate_report(&parsed_report(&value), &coupon_spec())
            .expect_err("a missing T-divider must fail");
        assert!(error.to_string().contains("T-divider"), "{error}");

        let mut value = coupon_report_value();
        value["coupon"]["wall"] = json!("");
        let error = validate_report(&parsed_report(&value), &coupon_spec())
            .expect_err("an unnamed wall must fail");
        assert!(error.to_string().contains("wall"), "{error}");
    }

    fn artifact(dir: &Path, name: &str, bytes: &[u8]) {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().expect("artifact path has a parent"))
            .expect("create artifact directory");
        fs::write(path, bytes).expect("write artifact");
    }

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

    /// A successful publish copies the contract artifacts and nothing else.
    #[test]
    fn publish_copies_only_the_contract_artifacts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let out = temp.path().join("build/1x1-6U-minimal-v1");
        let target = temp.path().join("boxes/1x1-6U-minimal-v1");
        fs::create_dir_all(&target).expect("box folder");
        fs::write(target.join("spec.yaml"), "source of truth\n").expect("spec");
        artifact(&out, "box.step", b"STEP");
        artifact(&out, "body.stl", b"BODY");
        artifact(&out, "notes.md", b"Printed: 2026-10-01\n");
        artifact(&out, "box.FCStd", b"review");
        artifact(&out, "report.json", b"{}");
        artifact(&out, "box.py", b"generated");
        artifact(&out, "cad/build.py", b"master");

        publish_artifacts(&minimal_spec(), &out, &target).expect("publish");

        assert_eq!(
            names(&target),
            ["body.stl", "box.step", "notes.md", "spec.yaml"]
        );
        assert_eq!(fs::read(target.join("box.step")).unwrap(), b"STEP");
        assert_eq!(fs::read(target.join("body.stl")).unwrap(), b"BODY");
        assert_eq!(
            fs::read_to_string(target.join("spec.yaml")).unwrap(),
            "source of truth\n"
        );
    }

    /// The notes file is the human print record; publish never overwrites it.
    #[test]
    fn publish_keeps_a_handwritten_notes_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let out = temp.path().join("build/1x1-6U-minimal-v1");
        let target = temp.path().join("boxes/1x1-6U-minimal-v1");
        fs::create_dir_all(&target).expect("box folder");
        fs::write(target.join("notes.md"), "Printed: 2026-10-01\n").expect("handwritten notes");
        artifact(&out, "box.step", b"STEP");
        artifact(&out, "body.stl", b"BODY");
        artifact(&out, "notes.md", b"generated placeholder\n");

        publish_artifacts(&minimal_spec(), &out, &target).expect("publish");

        assert_eq!(
            fs::read_to_string(target.join("notes.md")).unwrap(),
            "Printed: 2026-10-01\n"
        );
    }

    /// A build without notes gets a spec-derived placeholder before the copy.
    #[test]
    fn publish_generates_notes_from_the_spec_when_absent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let out = temp.path().join("build/1x1-6U-minimal-v1");
        let target = temp.path().join("boxes/1x1-6U-minimal-v1");
        fs::create_dir_all(&target).expect("box folder");
        artifact(&out, "box.step", b"STEP");
        artifact(&out, "body.stl", b"BODY");

        publish_artifacts(&minimal_spec(), &out, &target).expect("publish");

        let notes = fs::read_to_string(target.join("notes.md")).expect("published notes");
        assert!(notes.starts_with("# 1x1-6U-minimal-v1\n"), "{notes}");
        assert!(notes.contains("Stack group: F"), "{notes}");
        assert!(notes.contains("\nPrinted:\n"), "{notes}");
        assert!(
            out.join("notes.md").is_file(),
            "the generated notes must live in the build directory too"
        );
    }

    /// Every required artifact is checked before the first copy.
    #[test]
    fn publish_refuses_missing_or_empty_artifacts_without_touching_the_target() {
        for (label, box_step) in [("missing", None), ("empty", Some(&b""[..]))] {
            let temp = tempfile::tempdir().expect("tempdir");
            let out = temp.path().join("build/1x1-6U-minimal-v1");
            let target = temp.path().join("boxes/1x1-6U-minimal-v1");
            fs::create_dir_all(&target).expect("box folder");
            fs::write(target.join("spec.yaml"), "source of truth\n").expect("spec");
            if let Some(bytes) = box_step {
                artifact(&out, "box.step", bytes);
            }
            artifact(&out, "body.stl", b"BODY");
            artifact(&out, "notes.md", b"Printed: 2026-10-01\n");

            let error = match publish_artifacts(&minimal_spec(), &out, &target) {
                Ok(()) => panic!("publish must fail for a {label} box.step"),
                Err(error) => error,
            };

            assert_eq!(error.exit_code(), crate::error::ExitCode::BuildFailure);
            assert!(error.to_string().contains("box.step"), "{error}");
            assert_eq!(names(&target), ["spec.yaml"]);
            assert_eq!(
                fs::read_to_string(target.join("spec.yaml")).unwrap(),
                "source of truth\n"
            );
        }
    }

    /// A lid spec additionally requires the lid print file.
    #[test]
    fn publish_requires_lid_stl_for_a_lid_spec() {
        let temp = tempfile::tempdir().expect("tempdir");
        let out = temp.path().join("build/4x4-6U-lid-v1");
        let target = temp.path().join("boxes/4x4-6U-lid-v1");
        fs::create_dir_all(&target).expect("box folder");
        artifact(&out, "box.step", b"STEP");
        artifact(&out, "body.stl", b"BODY");
        artifact(&out, "notes.md", b"Printed:\n");

        let error = publish_artifacts(&lid_spec(), &out, &target).expect_err("publish must fail");

        assert_eq!(error.exit_code(), crate::error::ExitCode::BuildFailure);
        assert!(error.to_string().contains("lid.stl"), "{error}");
        assert_eq!(names(&target), Vec::<String>::new());

        artifact(&out, "lid.stl", b"LID");
        publish_artifacts(&lid_spec(), &out, &target).expect("publish with the lid");
        assert_eq!(
            names(&target),
            ["body.stl", "box.step", "lid.stl", "notes.md"]
        );
    }

    /// The spec must live at `<boxes>/<id>/spec.yaml`.
    #[test]
    fn publish_target_requires_a_parent_named_after_the_id() {
        let temp = tempfile::tempdir().expect("tempdir");
        let matching = temp.path().join("boxes/1x1-6U-minimal-v1/spec.yaml");
        assert_eq!(
            publish_target(&matching, "1x1-6U-minimal-v1").expect("matching layout"),
            temp.path().join("boxes/1x1-6U-minimal-v1")
        );

        let mismatched = temp.path().join("boxes/wrong-folder/spec.yaml");
        let error = publish_target(&mismatched, "1x1-6U-minimal-v1")
            .expect_err("mismatched folder must fail");

        assert_eq!(error.exit_code(), crate::error::ExitCode::Failure);
        let message = error.to_string();
        assert!(message.contains("boxes/<id>/spec.yaml"), "{message}");
        assert!(message.contains("1x1-6U-minimal-v1"), "{message}");
        assert!(message.contains("wrong-folder"), "{message}");

        let bare = Path::new("spec.yaml");
        assert!(publish_target(bare, "1x1-6U-minimal-v1").is_err());
    }
}
