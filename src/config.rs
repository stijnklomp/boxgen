//! Typed configuration for the local CLI.
//!
//! The server template read `APP_ENV` and infrastructure endpoints from the
//! environment. A spec compiler needs far less: a log filter and the FreeCAD
//! launcher. Both are parsed and validated here, once, instead of scattered
//! `env::var` reads in command code.

use std::path::PathBuf;

use tracing::level_filters::LevelFilter;

use crate::error::BoxgenError;

/// Environment variable selecting the stderr log level.
pub const ENV_LOG: &str = "BOXGEN_LOG";

/// Environment variable overriding the FreeCAD launcher.
pub const ENV_FREECADCMD: &str = "BOXGEN_FREECADCMD";

/// Environment variable overriding the expected FreeCAD release.
pub const ENV_FREECAD_VERSION: &str = "BOXGEN_EXPECT_FREECAD_VERSION";

/// Launcher used when [`ENV_FREECADCMD`] is unset.
pub const DEFAULT_FREECADCMD: &str = "freecadcmd";

/// FreeCAD release the build pipeline expects when
/// [`ENV_FREECAD_VERSION`] is unset. The `justfile` passes its pin here.
pub const DEFAULT_FREECAD_VERSION: &str = "1.0.0";

const LOG_VALUES: &str = "trace, debug, info, warn, error or off";

/// Runtime configuration for one boxgen invocation.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Maximum level emitted to stderr (`BOXGEN_LOG`, default `warn`).
    pub log_level: LevelFilter,
    /// Headless FreeCAD launcher (`BOXGEN_FREECADCMD`, default `freecadcmd`).
    pub freecadcmd: PathBuf,
    /// FreeCAD release the build verifies (`BOXGEN_EXPECT_FREECAD_VERSION`,
    /// default `1.0.0`).
    pub expected_freecad_version: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: LevelFilter::WARN,
            freecadcmd: PathBuf::from(DEFAULT_FREECADCMD),
            expected_freecad_version: DEFAULT_FREECAD_VERSION.to_owned(),
        }
    }
}

impl Config {
    /// Loads configuration from the process environment.
    pub fn load() -> Result<Self, BoxgenError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Parses configuration from a `key -> value` lookup.
    ///
    /// Unit tests use this seam instead of mutating the process environment.
    pub fn from_lookup(env: impl Fn(&str) -> Option<String>) -> Result<Self, BoxgenError> {
        let log_level = match env(ENV_LOG) {
            Some(value) => parse_log_level(&value)?,
            None => LevelFilter::WARN,
        };

        let freecadcmd = match env(ENV_FREECADCMD) {
            Some(value) if !value.trim().is_empty() => PathBuf::from(value.trim()),
            Some(_) => {
                return Err(BoxgenError::Environment {
                    message: format!("{ENV_FREECADCMD} is empty; expected a path to `freecadcmd`"),
                });
            }
            None => PathBuf::from(DEFAULT_FREECADCMD),
        };

        let expected_freecad_version = match env(ENV_FREECAD_VERSION) {
            Some(value) if !value.trim().is_empty() => value.trim().to_owned(),
            Some(_) => {
                return Err(BoxgenError::Environment {
                    message: format!(
                        "{ENV_FREECAD_VERSION} is empty; expected a FreeCAD release like {DEFAULT_FREECAD_VERSION}"
                    ),
                });
            }
            None => DEFAULT_FREECAD_VERSION.to_owned(),
        };

        Ok(Self {
            log_level,
            freecadcmd,
            expected_freecad_version,
        })
    }
}

fn parse_log_level(value: &str) -> Result<LevelFilter, BoxgenError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "trace" => Ok(LevelFilter::TRACE),
        "debug" => Ok(LevelFilter::DEBUG),
        "info" => Ok(LevelFilter::INFO),
        "warn" => Ok(LevelFilter::WARN),
        "error" => Ok(LevelFilter::ERROR),
        "off" => Ok(LevelFilter::OFF),
        _ => Err(BoxgenError::Environment {
            message: format!("invalid {ENV_LOG} value {value:?}; expected {LOG_VALUES}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_need_no_environment() {
        let config = Config::from_lookup(|_| None).unwrap();

        assert_eq!(config, Config::default());
        assert_eq!(config.log_level, LevelFilter::WARN);
        assert_eq!(config.freecadcmd, PathBuf::from("freecadcmd"));
        assert_eq!(config.expected_freecad_version, "1.0.0");
    }

    #[test]
    fn expected_freecad_version_is_trimmed_and_overridable() {
        let config = Config::from_lookup(|key| match key {
            ENV_FREECAD_VERSION => Some(" 1.1.0 ".to_owned()),
            _ => None,
        })
        .unwrap();

        assert_eq!(config.expected_freecad_version, "1.1.0");
    }

    #[test]
    fn empty_expected_freecad_version_is_rejected() {
        let error = Config::from_lookup(|key| match key {
            ENV_FREECAD_VERSION => Some("   ".to_owned()),
            _ => None,
        })
        .unwrap_err();

        assert_eq!(error.exit_code(), crate::error::ExitCode::Environment);
        assert!(error.to_string().contains(ENV_FREECAD_VERSION), "{error}");
        assert!(error.to_string().contains("1.0.0"), "{error}");
    }

    #[test]
    fn log_level_accepts_case_and_whitespace() {
        let config = Config::from_lookup(|key| match key {
            ENV_LOG => Some(" Debug ".to_owned()),
            _ => None,
        })
        .unwrap();

        assert_eq!(config.log_level, LevelFilter::DEBUG);
    }

    #[test]
    fn invalid_log_level_is_an_environment_error() {
        let error = Config::from_lookup(|key| match key {
            ENV_LOG => Some("verbose".to_owned()),
            _ => None,
        })
        .unwrap_err();

        assert_eq!(error.exit_code(), crate::error::ExitCode::Environment);
        assert!(error.to_string().contains("BOXGEN_LOG"), "{error}");
        assert!(error.to_string().contains("trace, debug"), "{error}");
    }

    #[test]
    fn freecadcmd_override_is_trimmed() {
        let config = Config::from_lookup(|key| match key {
            ENV_FREECADCMD => Some(" /opt/freecad/bin/freecadcmd ".to_owned()),
            _ => None,
        })
        .unwrap();

        assert_eq!(
            config.freecadcmd,
            PathBuf::from("/opt/freecad/bin/freecadcmd")
        );
    }

    #[test]
    fn empty_freecadcmd_is_rejected() {
        let error = Config::from_lookup(|key| match key {
            ENV_FREECADCMD => Some("   ".to_owned()),
            _ => None,
        })
        .unwrap_err();

        assert_eq!(error.exit_code(), crate::error::ExitCode::Environment);
        assert!(error.to_string().contains("BOXGEN_FREECADCMD"), "{error}");
    }
}
