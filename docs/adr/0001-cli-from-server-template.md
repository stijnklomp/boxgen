# CLI from the server template

`boxgen` started from `stijnklomp/rust-template`, a single-binary HTTP service template (axum, tokio, Postgres/sqlx, Valkey, RabbitMQ, pact, OpenAPI, compose, k8s). The generator contract needs the opposite shape: a local, offline spec compiler whose only external process is `freecadcmd`.

**Kept from the template:** Rust 2024 and the pinned 1.99.1 toolchain; `main.rs` wiring only; typed config parsed once and validated at startup; one error contract; `tracing` logging; `just` as the command surface; rustfmt/clippy gates with `-D warnings`; the commands-first `AGENTS.md`, glossary, ADR, and agent-doc conventions.

**Removed:** every server artifact and dependency — HTTP routes and middleware, OpenAPI, sqlx/Postgres, Valkey/Redis, RabbitMQ/lapin, migrations, pact contracts and acceptance tests, compose services, k8s manifests, the notes demo slice, and the server ADRs/PORTING notes.

**Adapted:**

- The service logged JSON to stdout. A CLI reserves stdout for reports, so logs are human-readable on stderr and `BOXGEN_LOG` sets the level.
- The service used cargo-nextest, sqlx offline checks, pact, cargo-deny, and SBOM recipes. The CLI gate is plain `cargo test` plus fmt and clippy; supply-chain and FreeCAD tooling are separate tickets, not baseline requirements.
- Configuration shrank from `APP_ENV`/`DATABASE_URL`/broker endpoints to `BOXGEN_LOG` and `BOXGEN_FREECADCMD`.
- The error contract became process exit codes 0/2/3/4 from the generator contract, with 1 reserved for stubs during the bootstrap. `BoxgenError` is the only error type commands return.

The frozen SYS-1.0 standard and generator contract stay authoritative and are not vendored here; the Rust side is deliberately thin.
