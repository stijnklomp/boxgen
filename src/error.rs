//! The one error contract for boxgen.
//!
//! Every command returns [`BoxgenError`]; `main` is the only place that turns
//! it into a process exit code. The contract codes are fixed by the generator
//! contract (`docs/generator.md` §5): 0 success, 2 invalid spec, 3 geometry or
//! build failure, 4 environment failure.

use std::path::PathBuf;

use thiserror::Error;

/// Stable process exit codes from the generator contract.
///
/// [`ExitCode::Failure`] (1) is not a contract failure code: it marks a
/// command that this bootstrap declares but has not implemented yet, and any
/// other failure that does not fit the four contract cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    /// Command succeeded.
    Success = 0,
    /// Command is not implemented yet, or failed without a contract code.
    Failure = 1,
    /// The spec is unreadable, malformed, or violates a validation rule.
    InvalidSpec = 2,
    /// Geometry generation or export failed.
    BuildFailure = 3,
    /// Required environment is missing or wrong (for example `freecadcmd`).
    Environment = 4,
}

impl ExitCode {
    /// Numeric code passed to the process.
    pub const fn code(self) -> u8 {
        self as u8
    }
}

/// Every failure a boxgen command can report.
#[derive(Debug, Error)]
pub enum BoxgenError {
    /// A command from the contract that this bootstrap does not implement yet.
    #[error("`boxgen {command}` is not implemented yet")]
    NotImplemented {
        /// Contract command name, as typed by the user.
        command: &'static str,
    },

    /// A spec could not be read, parsed, or validated.
    #[error("invalid spec `{path}`: {message}")]
    InvalidSpec {
        /// Spec file that failed.
        path: PathBuf,
        /// Precise rule or parser message.
        message: String,
    },

    /// FreeCAD failed to build or export the model.
    #[error("build failed for `{id}`: {message}")]
    Build {
        /// Box ID being built.
        id: String,
        /// Geometry or export failure.
        message: String,
    },

    /// The local toolchain or configuration is unusable.
    #[error("environment error: {message}")]
    Environment {
        /// What is missing or wrong, and what was expected.
        message: String,
    },
}

impl BoxgenError {
    /// Convenience constructor for an unimplemented contract command.
    pub const fn not_implemented(command: &'static str) -> Self {
        Self::NotImplemented { command }
    }

    /// The single mapping from error to process exit code.
    pub const fn exit_code(&self) -> ExitCode {
        match self {
            Self::NotImplemented { .. } => ExitCode::Failure,
            Self::InvalidSpec { .. } => ExitCode::InvalidSpec,
            Self::Build { .. } => ExitCode::BuildFailure,
            Self::Environment { .. } => ExitCode::Environment,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_exit_codes_are_stable() {
        assert_eq!(ExitCode::Success.code(), 0);
        assert_eq!(ExitCode::InvalidSpec.code(), 2);
        assert_eq!(ExitCode::BuildFailure.code(), 3);
        assert_eq!(ExitCode::Environment.code(), 4);
    }

    #[test]
    fn every_error_maps_to_its_contract_exit_code() {
        let invalid_spec = BoxgenError::InvalidSpec {
            path: PathBuf::from("boxes/example/spec.yaml"),
            message: "footprint side 5 is outside 1..=4".to_owned(),
        };
        assert_eq!(invalid_spec.exit_code(), ExitCode::InvalidSpec);

        let build = BoxgenError::Build {
            id: "4x4-6U-example-v1".to_owned(),
            message: "freecadcmd exited with status 1".to_owned(),
        };
        assert_eq!(build.exit_code(), ExitCode::BuildFailure);

        let environment = BoxgenError::Environment {
            message: "freecadcmd not found".to_owned(),
        };
        assert_eq!(environment.exit_code(), ExitCode::Environment);

        let unimplemented = BoxgenError::not_implemented("validate");
        assert_eq!(unimplemented.exit_code(), ExitCode::Failure);
    }

    #[test]
    fn unimplemented_error_names_the_command() {
        let error = BoxgenError::not_implemented("coupon");
        assert_eq!(error.to_string(), "`boxgen coupon` is not implemented yet");
    }
}
