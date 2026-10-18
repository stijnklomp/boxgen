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

/// Environment variable selecting the local UI port (`boxgen serve`).
pub const ENV_UI_PORT: &str = "BOXGEN_UI_PORT";

/// Environment variable carrying the host path shown for the mounted
/// workspace (`boxgen serve`).
pub const ENV_UI_HOST_WORKDIR: &str = "BOXGEN_UI_HOST_WORKDIR";

/// Launcher used when [`ENV_FREECADCMD`] is unset.
pub const DEFAULT_FREECADCMD: &str = "freecadcmd";

/// FreeCAD release the build pipeline expects when
/// [`ENV_FREECAD_VERSION`] is unset. The `justfile` passes its pin here.
pub const DEFAULT_FREECAD_VERSION: &str = "1.0.0";

/// Port the local UI binds on `127.0.0.1` when [`ENV_UI_PORT`] is unset
/// (ADR-0002). `--port` overrides both.
pub const DEFAULT_UI_PORT: u16 = 8787;

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
    /// Port `boxgen serve` binds on `127.0.0.1` (`BOXGEN_UI_PORT`, default
    /// [`DEFAULT_UI_PORT`]); `--port` overrides it.
    pub ui_port: u16,
    /// Host path of the mounted workspace (`BOXGEN_UI_HOST_WORKDIR`), shown
    /// for displayed paths; `None` means the process working directory.
    pub ui_host_workdir: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: LevelFilter::WARN,
            freecadcmd: PathBuf::from(DEFAULT_FREECADCMD),
            expected_freecad_version: DEFAULT_FREECAD_VERSION.to_owned(),
            ui_port: DEFAULT_UI_PORT,
            ui_host_workdir: None,
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

        let ui_port = match env(ENV_UI_PORT) {
            Some(value) => parse_ui_port(&value)?,
            None => DEFAULT_UI_PORT,
        };

        let ui_host_workdir = match env(ENV_UI_HOST_WORKDIR) {
            Some(value) if !value.trim().is_empty() => Some(PathBuf::from(value.trim())),
            Some(_) => {
                return Err(BoxgenError::Environment {
                    message: format!(
                        "{ENV_UI_HOST_WORKDIR} is empty; expected the host path of the mounted workspace"
                    ),
                });
            }
            None => None,
        };

        Ok(Self {
            log_level,
            freecadcmd,
            expected_freecad_version,
            ui_port,
            ui_host_workdir,
        })
    }
}

fn parse_ui_port(value: &str) -> Result<u16, BoxgenError> {
    match value.trim().parse::<u16>() {
        Ok(port) if port > 0 => Ok(port),
        _ => Err(BoxgenError::Environment {
            message: format!(
                "invalid {ENV_UI_PORT} value {value:?}; expected a TCP port between 1 and 65535"
            ),
        }),
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

    #[test]
    fn ui_port_defaults_and_parses() {
        let config = Config::from_lookup(|_| None).unwrap();
        assert_eq!(config.ui_port, DEFAULT_UI_PORT);
        assert_eq!(config.ui_host_workdir, None);

        let config = Config::from_lookup(|key| match key {
            ENV_UI_PORT => Some(" 9001 ".to_owned()),
            ENV_UI_HOST_WORKDIR => Some(" /home/me/boxgen ".to_owned()),
            _ => None,
        })
        .unwrap();

        assert_eq!(config.ui_port, 9001);
        assert_eq!(
            config.ui_host_workdir,
            Some(PathBuf::from("/home/me/boxgen"))
        );
    }

    #[test]
    fn invalid_ui_ports_are_environment_errors() {
        for value in ["0", "-1", "70000", "eight", ""] {
            let error = Config::from_lookup(|key| match key {
                ENV_UI_PORT => Some(value.to_owned()),
                _ => None,
            })
            .unwrap_err();

            assert_eq!(error.exit_code(), crate::error::ExitCode::Environment);
            assert!(error.to_string().contains(ENV_UI_PORT), "{error}");
        }
    }

    #[test]
    fn empty_ui_host_workdir_is_rejected() {
        let error = Config::from_lookup(|key| match key {
            ENV_UI_HOST_WORKDIR => Some("  ".to_owned()),
            _ => None,
        })
        .unwrap_err();

        assert_eq!(error.exit_code(), crate::error::ExitCode::Environment);
        assert!(error.to_string().contains(ENV_UI_HOST_WORKDIR), "{error}");
    }
}
