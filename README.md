# boxgen

`boxgen` is the **SYS-1.0 spec compiler**: it turns a YAML box spec into a FreeCAD model and printable files.

```
spec.yaml ──validate──▶ build/<id>/box.py ──freecadcmd──▶ box.step + body.stl + lid.stl
     │                         │
     │                         └── imports the FreeCAD Python master in cad/
     └── footprint, height, division tree, lid
```

FreeCAD is the geometry engine. `boxgen` validates specs, renders the FreeCAD Python build script, runs it headless with `freecadcmd`, reports dimensions and warnings, and maintains the box registry. It never implements a CAD kernel and it never generates meshes itself.

> **Status: bootstrap.** The crate, CLI surface, typed config, logging, error contract, and Docker-first gates are in place. The five contract commands are stubs that return a clear "not implemented" error until their feature tickets land. The binary is `boxgen`, and `boxgen --help` already lists the full contract.

## Docker-first quick start

The host needs no Rust, cargo, or `just`. Docker is the only prerequisite for the gate; FreeCAD is only needed for real builds and is pinned by ticket #3.

Build the dev image once:

```bash
docker build -f docker/dev.Dockerfile -t boxgen/dev:rust-1.99.1 .
```

Run the gate exactly as later CI will:

```bash
docker run --rm -u "$(id -u):$(id -g)" \
  -e CARGO_HOME=/tmp/cargo -e CARGO_TARGET_DIR=/tmp/target \
  -v "$PWD":/app -w /app boxgen/dev:rust-1.99.1 just check
```

Run the CLI:

```bash
docker run --rm -u "$(id -u):$(id -g)" \
  -e CARGO_HOME=/tmp/cargo -e CARGO_TARGET_DIR=/tmp/target \
  -v "$PWD":/app -w /app boxgen/dev:rust-1.99.1 cargo run -- --help
```

Inside the image, `just` alone lists the recipes: `check`, `fmt`, `fmt-check`, `lint`, `test`, `build`, `run`, `docker-build-dev`, `docker-check`.

The Rust toolchain is pinned to **1.99.1** in `rust-toolchain.toml` and in the dev image tag; bumps are deliberate commits. The FreeCAD version pin belongs to ticket #3, not this bootstrap.

## CLI contract

| Command | Purpose | Status |
| --- | --- | --- |
| `boxgen validate <spec>` | Parse and validate a spec; print a human-readable report. | Stub (ticket #4) |
| `boxgen build <spec> [--out DIR] [--publish]` | Validate, write `build/<id>/box.py`, run `freecadcmd`, export STEP/STL. | Stub (tickets #6-#9) |
| `boxgen divide <spec> [--path P] [--axis x\|y] [--at MM]` | Edit one split in a spec's division tree in place. | Stub (ticket #12) |
| `boxgen index [--boxes DIR]` | Regenerate `boxes/INDEX.md`. | Stub (ticket #11) |
| `boxgen coupon [--out DIR]` | Generate the calibration coupon build. | Stub (ticket #10) |

Exit codes are part of the contract:

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Usage error, not implemented in this bootstrap, or unclassified failure; not a contract failure code |
| 2 | Invalid spec (unreadable, malformed, or a validation rule failed) |
| 3 | Geometry or build failure (FreeCAD error, export failure) |
| 4 | Environment error (missing or wrong `freecadcmd`, bad configuration) |

The mapping lives in exactly one place: `src/error.rs`. Commands return `BoxgenError`; only `src/main.rs` converts it to a process exit code.

## Configuration and logging

| Variable | Default | Meaning |
| --- | --- | --- |
| `BOXGEN_LOG` | `warn` | Stderr log level: `trace`, `debug`, `info`, `warn`, `error`, or `off`. |
| `BOXGEN_FREECADCMD` | `freecadcmd` | Headless FreeCAD launcher used by `build` and `coupon`. |

Typed config is parsed once in `src/config.rs`. Reports go to stdout; diagnostics go to stderr as human-readable `tracing` output, so command output can be piped without JSON noise.

## SYS-1.0 specification

The authoritative spec is a **separate local repository** (no remote yet), checked out beside this repository. The generator contract is this crate's blueprint.

| Document | Path from a normal sibling checkout |
| --- | --- |
| Generator contract | `../research/3d-printed-stackable-portable-storage/docs/generator.md` |
| Design standard (wins on conflict) | `../research/3d-printed-stackable-portable-storage/docs/standard.md` |
| Workflow | `../research/3d-printed-stackable-portable-storage/docs/workflow.md` |
| Calibration procedure | `../research/3d-printed-stackable-portable-storage/docs/calibration.md` |
| Box spec template | `../research/3d-printed-stackable-portable-storage/templates/box-spec.yaml` |
| Box registry | `../research/3d-printed-stackable-portable-storage/boxes/` |

Key contract points:

- A spec fixes the system version (`SYS-1.0`), box ID, footprint (1 × 1 to 4 × 4 units), height (3U, 6U, 9U, or 12U), lid flag, notes, and the recursive division tree. Wall thickness, radii, lip dimensions, and print settings are fixed by the standard and cannot appear in a spec.
- The division tree is the only stored compartment form. Dividers are 2.0 mm, leaves are at least 12 mm (warn below 15 mm), and nested splits resolve from the parent's origin.
- `build` writes `build/<id>/box.py`, `box.FCStd`, `box.step`, `body.stl`, `lid.stl` (when `lid: true`), and `report.json`. Only `--publish` copies accepted artifacts into `boxes/<id>/`.
- FreeCAD master modules live in `cad/` and expose `build(spec, out_dir) -> dict`; the Rust side only orchestrates.

The standard wins if it and the generator contract disagree.

## Repository layout

```
src/
  main.rs        CLI entry: parse, configure, log, map errors to exit codes
  cli.rs         clap definitions, help text, and command dispatch
  spec.rs        YAML schema, parsing, validation (ticket #4)
  tree.rs        division tree, leaf sizes, validation, divide helper
  build.rs       build orchestration and freecadcmd invocation
  index.rs       boxes/INDEX.md generation
  config.rs      typed BOXGEN_* configuration
  logging.rs     stderr tracing subscriber
  error.rs       the one error contract and exit-code mapping
cad/             FreeCAD Python master (planned: tickets #5-#9)
templates/       box-spec.yaml, the copyable spec skeleton
tests/           CLI and integration tests
docker/          Docker-first dev/gate image
```

## Gates and tests

`just check` is the fast gate and needs no infrastructure: `fmt-check`, `lint` (clippy with `-D warnings`), then `test` (unit, integration, and doctests). There is no database, broker, cache, HTTP server, or compose stack.

Current tests pin:

- `boxgen --help` and every subcommand help;
- every contract command name;
- the bootstrap "not implemented" error for all five stubs;
- the 0/2/3/4 exit-code mapping in `src/error.rs`;
- typed config parsing and the committed YAML template.

CI files are deliberately out of scope for this ticket; ticket #3 adds the FreeCAD image and CI running the same gate.

## Documentation

- `AGENTS.md` — commands, rules, crate map, and pitfalls for agents working here.
- `GLOSSARY.md` — repository vocabulary; the spec repository's glossary stays authoritative for domain terms.
- `docs/adr/` — architecture decisions for this crate.
- `docs/agents/` — issue tracker, triage labels, and domain-doc conventions.

## License

MIT — see `LICENSE`.
