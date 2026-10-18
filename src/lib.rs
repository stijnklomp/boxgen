//! boxgen is the SYS-1.0 spec compiler.
//!
//! It validates a YAML box spec, renders a FreeCAD Python build script, runs it
//! under `freecadcmd`, and maintains the box registry. FreeCAD is the geometry
//! engine; this crate never implements a CAD kernel.
//!
//! The command surface is in [`cli`], the process error contract in [`error`],
//! and the fixed module responsibilities in [`spec`], [`tree`], [`build`] and
//! [`index`]. The v2 local web UI is [`serve`], one non-contract command
//! (ADR-0002). The authoritative generator contract is the SYS-1.0 spec
//! vendored at `docs/spec/`: `docs/spec/docs/generator.md`.

pub mod build;
pub mod cli;
pub mod config;
pub mod error;
pub mod index;
pub mod logging;
pub mod serve;
pub mod spec;
pub mod tree;
