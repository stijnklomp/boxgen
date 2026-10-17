//! Build orchestration and `freecadcmd` invocation.
//!
//! Contract: `docs/generator.md` §5 (CLI contract, artifacts, publish) and §6
//! (FreeCAD master contract). `build` validates a spec, writes
//! `build/<id>/box.py`, runs it headless through `freecadcmd`, and collects
//! `box.FCStd`, `box.step`, `body.stl`, `lid.stl`, and `report.json`.
//! `--publish` copies the accepted artifacts into `boxes/<id>/`.
//!
//! The generator itself lands with tickets #6-#10. No geometry is implemented
//! in Rust: this module only orchestrates the Python master.

use std::path::Path;

use crate::config::Config;
use crate::error::BoxgenError;

/// Builds one spec and optionally publishes the accepted artifacts.
pub fn build(
    _spec: &Path,
    _out: Option<&Path>,
    _publish: bool,
    _config: &Config,
) -> Result<(), BoxgenError> {
    Err(BoxgenError::not_implemented("build"))
}

/// Generates the calibration coupon through the normal build path.
pub fn coupon(_out: Option<&Path>, _config: &Config) -> Result<(), BoxgenError> {
    Err(BoxgenError::not_implemented("coupon"))
}
