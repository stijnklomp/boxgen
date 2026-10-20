//! The CLI surface fixed by the generator contract.
//!
//! Command names, required arguments, and help text are the public interface;
//! keep them stable. Command implementations live in [`crate::spec`],
//! [`crate::tree`], [`crate::build`] and [`crate::index`], so this module stays
//! a parser and dispatcher.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::build;
use crate::config::Config;
use crate::error::BoxgenError;
use crate::index;
use crate::serve;
use crate::spec;
use crate::tree;

/// Compile YAML box specs into FreeCAD models and print files.
#[derive(Debug, Parser)]
#[command(
    name = "boxgen",
    version,
    about = "Compile SYS-2.0 YAML box specs into FreeCAD models and print files",
    long_about = "boxgen is the SYS-2.0 spec compiler.\n\n\
        It validates a YAML box spec, generates a FreeCAD Python build script, \
        runs it headless with freecadcmd, and maintains the box registry. \
        FreeCAD is the geometry engine; boxgen never generates meshes itself.",
    propagate_version = true
)]
pub struct Cli {
    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The five SYS-2.0 contract commands plus the v2 local UI.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Parse and validate a spec; print a human-readable report.
    Validate {
        /// Path to the YAML box spec.
        #[arg(value_name = "SPEC")]
        spec: PathBuf,
    },

    /// Validate, write build/<id>/box.py, run freecadcmd, and export STEP/STL.
    Build {
        /// Path to the YAML box spec.
        #[arg(value_name = "SPEC")]
        spec: PathBuf,

        /// Directory for generated artifacts (default: build/<id>).
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,

        /// Copy STEP, STLs, and notes.md into boxes/<id>/ after a successful build.
        #[arg(long)]
        publish: bool,
    },

    /// Edit one split in a spec's division tree in place.
    Divide {
        /// Path to the YAML box spec.
        #[arg(value_name = "SPEC")]
        spec: PathBuf,

        /// Path to the split in the tree, for example `root.near`.
        #[arg(long, value_name = "PATH")]
        path: Option<String>,

        /// Split axis: x is a vertical divider, y is a horizontal divider.
        #[arg(long, value_name = "x|y")]
        axis: Option<AxisArg>,

        /// Distance from the parent's origin edge to the near divider face, in mm.
        #[arg(long = "at", value_name = "MM")]
        at_mm: Option<u32>,
    },

    /// Regenerate boxes/INDEX.md from the box specs.
    Index {
        /// Directory containing the box registry.
        #[arg(long, value_name = "DIR", default_value = "boxes")]
        boxes: PathBuf,
    },

    /// Generate the calibration coupon build.
    Coupon {
        /// Directory for generated artifacts.
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
    },

    /// Start the local web UI for the spec (v2; ADR-0002).
    Serve {
        /// TCP port to bind on 127.0.0.1 (default: BOXGEN_UI_PORT or 8787).
        #[arg(long, value_name = "PORT", value_parser = clap::value_parser!(u16).range(1..))]
        port: Option<u16>,

        /// Open the UI in the default browser after the server starts.
        #[arg(long)]
        open: bool,
    },
}

/// Split axis accepted by `boxgen divide`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AxisArg {
    /// Vertical divider that moves left/right.
    X,
    /// Horizontal divider that moves up/down.
    Y,
}

impl AxisArg {
    /// The axis spelling used by the spec schema.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
        }
    }
}

/// Dispatches a parsed command to its module.
pub fn run(cli: Cli, config: &Config) -> Result<(), BoxgenError> {
    tracing::debug!(command = ?cli.command, "dispatching command");

    match cli.command {
        Command::Validate { spec: path } => spec::validate(&path),
        Command::Build {
            spec: path,
            out,
            publish,
        } => build::build(&path, out.as_deref(), publish, config),
        Command::Divide {
            spec,
            path,
            axis,
            at_mm,
        } => tree::divide(&spec, path.as_deref(), axis.map(AxisArg::as_str), at_mm),
        Command::Index { boxes } => index::generate(&boxes),
        Command::Coupon { out } => build::coupon(out.as_deref(), config),
        Command::Serve { port, open } => serve::serve(port, open, config),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn help_lists_every_contract_command() {
        let help = Cli::command().render_long_help().to_string();

        for command in ["validate", "build", "divide", "index", "coupon"] {
            assert!(help.contains(command), "help is missing {command}:\n{help}");
        }
    }

    #[test]
    fn build_accepts_out_and_publish() {
        let cli = Cli::try_parse_from([
            "boxgen",
            "build",
            "boxes/example/spec.yaml",
            "--out",
            "build/example",
            "--publish",
        ])
        .unwrap();

        match cli.command {
            Command::Build { spec, out, publish } => {
                assert_eq!(spec, PathBuf::from("boxes/example/spec.yaml"));
                assert_eq!(out, Some(PathBuf::from("build/example")));
                assert!(publish);
            }
            other => panic!("expected build, got {other:?}"),
        }
    }

    #[test]
    fn index_defaults_to_the_boxes_directory() {
        let cli = Cli::try_parse_from(["boxgen", "index"]).unwrap();

        match cli.command {
            Command::Index { boxes } => assert_eq!(boxes, PathBuf::from("boxes")),
            other => panic!("expected index, got {other:?}"),
        }
    }

    #[test]
    fn divide_accepts_axis_and_position() {
        let cli = Cli::try_parse_from([
            "boxgen",
            "divide",
            "boxes/example/spec.yaml",
            "--path",
            "root.near",
            "--axis",
            "y",
            "--at",
            "60",
        ])
        .unwrap();

        match cli.command {
            Command::Divide {
                path, axis, at_mm, ..
            } => {
                assert_eq!(path.as_deref(), Some("root.near"));
                assert_eq!(axis, Some(AxisArg::Y));
                assert_eq!(at_mm, Some(60));
            }
            other => panic!("expected divide, got {other:?}"),
        }
    }

    #[test]
    fn divide_rejects_an_unknown_axis() {
        let error =
            Cli::try_parse_from(["boxgen", "divide", "boxes/example/spec.yaml", "--axis", "z"])
                .unwrap_err();

        assert_eq!(error.kind(), clap::error::ErrorKind::InvalidValue);
    }

    #[test]
    fn every_contract_command_parses_its_required_spec() {
        for name in ["validate", "build", "divide"] {
            assert!(
                Cli::try_parse_from(["boxgen", name, "boxes/example/spec.yaml"]).is_ok(),
                "{name} must accept a spec path"
            );
        }

        assert!(Cli::try_parse_from(["boxgen", "index"]).is_ok());
        assert!(Cli::try_parse_from(["boxgen", "coupon"]).is_ok());
    }

    #[test]
    fn serve_accepts_port_and_open() {
        let cli = Cli::try_parse_from(["boxgen", "serve", "--port", "9001", "--open"]).unwrap();

        match cli.command {
            Command::Serve { port, open } => {
                assert_eq!(port, Some(9001));
                assert!(open);
            }
            other => panic!("expected serve, got {other:?}"),
        }

        let cli = Cli::try_parse_from(["boxgen", "serve"]).unwrap();
        match cli.command {
            Command::Serve { port, open } => {
                assert_eq!(port, None);
                assert!(!open);
            }
            other => panic!("expected serve, got {other:?}"),
        }
    }

    #[test]
    fn serve_rejects_port_zero() {
        let error = Cli::try_parse_from(["boxgen", "serve", "--port", "0"]).unwrap_err();

        assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
    }
}
