# AGENTS.md

`boxgen` is the **SYS-1.0 spec compiler**: YAML box spec → FreeCAD Python master → STEP/STL. It is a local CLI, not a service. Vocabulary: `GLOSSARY.md`. Decisions: `docs/adr/`. The authoritative generator contract is `docs/generator.md` in the SYS-1.0 spec repository (checked out beside this repo: `../research/3d-printed-stackable-portable-storage/`); the design standard wins if they disagree.

## Commands

The host has no Rust toolchain by design. Build the dev image once, then run every recipe inside it.

```bash
docker build -f docker/dev.Dockerfile -t boxgen/dev:rust-1.99.1 .

# The gate: fmt-check + lint (clippy -D warnings) + tests. No infrastructure.
docker run --rm -u "$(id -u):$(id -g)" \
  -e CARGO_HOME=/tmp/cargo -e CARGO_TARGET_DIR=/tmp/target \
  -v "$PWD":/app -w /app boxgen/dev:rust-1.99.1 just check
```

`just` alone lists the recipes. Inside the image:

| Recipe | Purpose |
| --- | --- |
| `just check` | The gate: `fmt-check`, `lint`, `test`. |
| `just fmt` / `just fmt-check` | Format / check formatting. |
| `just lint` | Clippy over all targets with warnings denied. |
| `just test` | Unit, integration, and doctests. |
| `just build` | Build the `boxgen` binary. |
| `just run -- --help` | Run the CLI. |
| `just docker-build-dev` | Build the dev/gate image. |
| `just docker-check` | Run `just check` in a fresh dev container. |

## Rules

- **FreeCAD is the only geometry engine.** Rust validates, renders the Python build script, invokes `freecadcmd`, and records results. Never implement mesh or CSG geometry in Rust.
- **The spec is the source of truth.** `spec.yaml` defines one box; `build/<id>/` is generated and never hand-edited; `boxes/<id>/` changes only through a successful `--publish`.
- **One error contract.** Return `BoxgenError` from commands; map it to a process code only in `src/error.rs`. Exit codes are fixed: 0 success, 2 invalid spec, 3 geometry/build failure, 4 environment. Code 1 covers usage errors and commands that are not implemented during the bootstrap; it is never a contract failure code.
- **No server infrastructure.** Do not add axum, a tokio server, sqlx, Postgres, Valkey/Redis, RabbitMQ/lapin, pact, utoipa/OpenAPI, migrations, k8s, or compose services. v1 is a local CLI; a web UI is explicitly a v2 non-goal.
- **Config lives in `src/config.rs`.** Add a typed setting there (`BOXGEN_*`), never an ad-hoc `env::var` in command code.
- **stdout is for reports, stderr is for logs.** Keep it that way so `validate` and `index` output can be piped.
- **Do not remove the contract commands from `--help`.** Their argument shapes are the public interface even while they are stubs. Implement them in their ticket; keep `src/cli.rs` a parser and dispatcher.
- **Keep the spec schema closed.** Wall thickness, radii, lip dimensions, and print settings must never become spec fields.
- **Run `just check` in the dev image before declaring work done.** CI is ticket #3; do not add a GitHub Actions workflow here.
- **Do not vendor the SYS-1.0 standard.** Point at the spec repository and keep this crate's docs about the generator.

## Crate map

- `src/main.rs` — thin entry: parse, load config, init logging, map `BoxgenError` to the exit code.
- `src/cli.rs` — clap definitions, help text, `AxisArg`, dispatch to the command modules.
- `src/spec.rs` — YAML schema, parsing, and validation (ticket #4). `SYSTEM_VERSION` lives here.
- `src/tree.rs` — division tree, constants, validation, and the in-place `divide` helper (tickets #4 and #12).
- `src/build.rs` — `build`/`coupon` orchestration: render `box.py`, invoke `freecadcmd`, collect artifacts (tickets #6-#10).
- `src/index.rs` — deterministic `boxes/INDEX.md` generation (ticket #11).
- `src/config.rs` — typed `BOXGEN_LOG` and `BOXGEN_FREECADCMD`.
- `src/logging.rs` — stderr `tracing` subscriber; stdout stays clean.
- `src/error.rs` — `BoxgenError` and `ExitCode`, the single exit-code mapping.
- `cad/` — FreeCAD Python master, added from ticket #5 onwards.
- `templates/box-spec.yaml` — copyable spec skeleton pinned by a YAML parse test.
- `tests/cli.rs` — process-level CLI contract tests.
- `docker/dev.Dockerfile` — Rust 1.99.1 + `just`; the Docker-first gate.

## Pitfalls

- The host has no cargo/rustc/just. If a command fails with "command not found", you forgot the container.
- `cargo fmt` and `clippy` come from the dev image. The plain `rust:1.99.1-slim-trixie` image has neither, so use `boxgen/dev:rust-1.99.1` for gates.
- The five commands intentionally exit 1 with "not implemented" until their tickets land. Do not make a stub exit 0; the integration test pins this bootstrap behaviour, and help tests pin the command names.
- `--publish` must never run before a successful build; do not populate `boxes/` from a failed run.
- The spec repository is local-only (no remote yet). If its location changes, update the pointers in `README.md`, this file, and `docs/agents/domain.md`.

## Agent docs

- Issue tracker: GitHub issues in `stijnklomp/boxgen` via `gh` — `docs/agents/issue-tracker.md`.
- Triage labels — `docs/agents/triage-labels.md`.
- Domain docs: single context — `GLOSSARY.md` + `docs/adr/` — `docs/agents/domain.md`.
