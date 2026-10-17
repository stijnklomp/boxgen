//! Pinned FreeCAD toolchain contract.
//!
//! Native development runs without FreeCAD, so this test skips unless
//! `BOXGEN_REQUIRE_FREECAD=1`. The Docker gate (`just check-docker`) and CI set
//! that variable inside the pinned image: a missing launcher or the wrong
//! FreeCAD release is then a hard failure, so the image tag cannot silently
//! drift from the toolchain the suite expects.

use std::process::Command;

/// Set to `1` by the gate image to make this test mandatory.
const REQUIRE_ENV: &str = "BOXGEN_REQUIRE_FREECAD";

/// Expected release, passed by the `test` recipe from the justfile pin.
const EXPECT_ENV: &str = "BOXGEN_EXPECT_FREECAD_VERSION";

/// Fallback when the test is run directly inside the image.
const DEFAULT_EXPECTED_VERSION: &str = "1.0.0";

#[test]
fn freecadcmd_reports_the_pinned_version_when_required() {
    if std::env::var(REQUIRE_ENV).ok().as_deref() != Some("1") {
        eprintln!(
            "skipping FreeCAD toolchain check: {REQUIRE_ENV}=1 is only set inside the pinned image"
        );
        return;
    }

    let expected = std::env::var(EXPECT_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_EXPECTED_VERSION.to_owned());

    let output = Command::new("freecadcmd")
        .arg("--version")
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "{REQUIRE_ENV}=1 but `freecadcmd --version` could not run: {error}; use `just check-docker`"
            )
        });

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "freecadcmd --version failed with {}:\n{stdout}{stderr}",
        output.status
    );
    assert!(
        stdout.contains(&expected) || stderr.contains(&expected),
        "freecadcmd reports the wrong FreeCAD release; expected {expected:?} in:\n{stdout}{stderr}"
    );
}
