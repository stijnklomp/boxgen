//! Division tree, leaf sizes, and validation.
//!
//! Contract: `docs/generator.md` §4. A node is either a leaf (an optional
//! compartment name) or a split on one axis into a near and a far child. The
//! tree is the only stored form; grid views are renderings.
//!
//! Tree types and the validation rules land with ticket #4, dividers with
//! ticket #8, and the in-place `divide` helper with ticket #12. This module
//! owns all of them so the schema and geometry code share one tree model.

use std::path::Path;

use crate::error::BoxgenError;

/// Every divider is 2.0 mm thick (contract §4).
pub const DIVIDER_MM: f64 = 2.0;

/// Smallest legal leaf dimension in millimetres (contract §4).
pub const MIN_LEAF_MM: u32 = 12;

/// Leaves below this size are valid but warned about (contract §4).
pub const WARN_LEAF_MM: u32 = 15;

/// Edits one split in a spec's division tree in place.
pub fn divide(
    _spec: &Path,
    _path: Option<&str>,
    _axis: Option<&str>,
    _at_mm: Option<u32>,
) -> Result<(), BoxgenError> {
    Err(BoxgenError::not_implemented("divide"))
}
