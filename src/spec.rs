//! YAML box-spec schema, parsing, and validation.
//!
//! Contract: `docs/generator.md` §3 (input schema), §7 (validation rules), and
//! §9 (non-goals). The spec is the source of truth for one body; wall
//! thickness, radii, lip dimensions and print settings are fixed by the
//! standard and must never be expressible in a spec.
//!
//! The typed schema and every validation rule land with ticket #4. Until then
//! [`validate`] parses the file's YAML and reports that command execution is not
//! implemented yet. Schema validation must stay in this module: the other
//! commands receive a parsed spec, never a raw file path.

use std::path::Path;

use crate::error::BoxgenError;

/// The only system version this compiler accepts (contract §3).
pub const SYSTEM_VERSION: &str = "SYS-1.0";

/// Parses and validates `spec`, printing a human-readable report.
pub fn validate(_spec: &Path) -> Result<(), BoxgenError> {
    Err(BoxgenError::not_implemented("validate"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_spec_template_is_valid_yaml() {
        // Syntax only: the schema rules arrive with ticket #4. This keeps the
        // template honest and exercises the YAML parser the compiler will use.
        let template = include_str!("../templates/box-spec.yaml");
        let document: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(template).expect("template must be valid YAML");

        assert_eq!(document["system"].as_str(), Some(SYSTEM_VERSION));
        assert!(
            document["divisions"].is_mapping(),
            "the template must contain a division tree"
        );
    }
}
