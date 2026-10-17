//! `boxes/INDEX.md` generation.
//!
//! Contract: `docs/generator.md` §5 and the spec repository's committed index
//! format. The index is generated from box specs, never edited by hand, and
//! lists every box with its footprint, height, contents, stack group, and print
//! date.
//!
//! Implemented with the registry ticket.

use std::path::Path;

use crate::error::BoxgenError;

/// Regenerates `boxes/INDEX.md` from the specs under `boxes`.
pub fn generate(_boxes: &Path) -> Result<(), BoxgenError> {
    Err(BoxgenError::not_implemented("index"))
}
