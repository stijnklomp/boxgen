# boxgen

`boxgen` is the **SYS-1.0 spec compiler**: it turns a YAML box spec into a FreeCAD model and printable files.

```
spec.yaml ──validate──▶ build/<id>/box.py ──freecadcmd──▶ box.step + body.stl + lid.stl
     │                         │
     │                         └── imports the FreeCAD Python master in cad/
     └── footprint, height, division tree, lid
```

FreeCAD is the geometry engine. `boxgen` validates specs, renders the FreeCAD Python build script, runs it headless with `freecadcmd`, reports dimensions and warnings, and maintains the box registry. It never implements a CAD kernel and it never generates meshes itself.

> **Status: all five contract commands implemented, the v2 local web UI shipped, and profiles pinned.** The crate, CLI surface, typed config, logging, error contract, Docker-first gates, `boxgen validate`, `boxgen divide`, the build path with the ticketed body shell (#6), the division-tree dividers (#8), the inset friction lid (#9), `boxgen coupon` (#10), `boxgen index` (#11), and `boxgen build --publish` (#11) are in place. `boxgen build` turns a spec into `box.py`, `box.FCStd`, `box.step`, `body.stl`, `lid.stl` (when `lid: true`), and `report.json` under the pinned FreeCAD; every split becomes a 2.0 mm divider fused into the body with 2.0 mm junction coves and a 0.4 mm top chamfer, and validation warnings (a leaf below 15 mm) flow into `report.json`; `--publish` copies the accepted files into the spec's box folder. `boxgen coupon` builds the calibration coupon (1 × 1 × 6U, T-divider, ribbed lid) through the same pipeline into `build/coupon`. `cad/gridfinity.py` vendors the pinned Gridfinity base and stacking-lip profiles, `cad/shell.py` + `cad/dividers.py` + `cad/build.py` assemble and export the body (shell, dividers, and lid), `cad/fillets.py` applies the standard's 2.0 -> 1.0 mm reduce-then-fail contract for internal fillets (recorded as `body.floor_fillet_mm`), and `cad/lid.py` builds the 2.0 mm plate with its 2.0 mm skirt, optional `lid_ribs`, and measured lid report. The v2 local web UI (`boxgen serve`, ADR-0002) is shipped as well: one axum + tokio binary serving the embedded frontend and the Rust API on `127.0.0.1` only, with the division editor, fit assistant, read-only system panel, and the one build pipeline with 3D preview, export, publish, and index; `scripts/boxgen-ui.sh` (or `just ui`) starts it in Docker with one command. The binary is `boxgen`, and `boxgen --help` lists the full contract plus the one non-contract v2 command.

## Docker-first quick start

The full gate needs only Docker. The host needs no Rust, cargo, `just`, or
FreeCAD: `docker/freecad.Dockerfile` builds the pinned toolchain image
`boxgen/freecad:1.0.0` from Rust 1.99.1, `just`, and headless FreeCAD 1.0.0.

Build the image and run the authoritative gate:

```bash
just check-docker
```

`check-docker` builds the image from the committed Dockerfile and runs
`just check` inside it with `BOXGEN_REQUIRE_FREECAD=1`, so the Rust suite and
the pinned FreeCAD assertion run against the same toolchain as CI. Without
`just` on the host, the equivalent commands are:

```bash
docker build --build-arg FREECAD_VERSION=1.0.0 \
  -f docker/freecad.Dockerfile -t boxgen/freecad:1.0.0 .

docker run --rm -u "$(id -u):$(id -g)" \
  -e HOME=/tmp/boxgen-home \
  -e FREECAD_USER_HOME=/tmp/boxgen-freecad \
  -e CARGO_HOME=/tmp/boxgen-cargo \
  -e CARGO_TARGET_DIR=/tmp/boxgen-target \
  -e BOXGEN_REQUIRE_FREECAD=1 \
  -v "$PWD":/app -w /app \
  boxgen/freecad:1.0.0 just check
```

`just check` is the fast native gate for hosts that already have Rust 1.99.1
and `just`: `fmt-check`, `lint` (clippy with `-D warnings`), then `test` (unit,
integration, and doctests). It runs the Rust suite only; the FreeCAD
environment test skips unless `BOXGEN_REQUIRE_FREECAD=1`. Geometry work must use
`just check-docker` (or CI), because only the image carries FreeCAD 1.0.0.

Inside the image, `just` alone lists the recipes: `check`, `check-docker`,
`docker-build`, `freecad-smoke`, `fmt`, `fmt-check`, `lint`, `test`, `build`,
`run`, `ui`.

### Pinned FreeCAD version

The release is pinned by one variable in `justfile`:

```just
freecad_version := "1.0.0"
```

It is the Docker build argument, the image tag (`boxgen/freecad:1.0.0`), and
the version the integration test expects. CI reads the same variable from the
justfile, so the pin cannot drift between the local gate and CI. To bump
FreeCAD, change that one line in a deliberate commit, rebuild, and re-run the
geometry tests; the new image tag makes the upgrade visible. FreeCAD updates
can change geometry, so a bump is a reviewed change, never an implicit
`latest` pull.

The Rust toolchain is pinned to **1.99.1** in `rust-toolchain.toml` and in the
image base; bumps are deliberate commits too.

## CLI contract

| Command | Purpose | Status |
| --- | --- | --- |
| `boxgen validate <spec>` | Parse and validate a spec; print a human-readable report. | Implemented (ticket #4) |
| `boxgen build <spec> [--out DIR] [--publish]` | Validate, write `build/<id>/box.py`, run `freecadcmd`, build the body (shell + division-tree dividers, plus the lid when `lid: true`), export STEP/STL, and publish accepted files into `boxes/<id>/`. | Implemented (tickets #6, #8, #9, #11) |
| `boxgen divide <spec> [--path P] [--axis x\|y] [--at MM]` | Edit one split in a spec's division tree in place. | Implemented (ticket #12) |
| `boxgen index [--boxes DIR]` | Regenerate `boxes/INDEX.md`. | Implemented (ticket #11) |
| `boxgen coupon [--out DIR]` | Generate the calibration coupon build (default `build/coupon`). | Implemented (ticket #10) |
| `boxgen serve [--port PORT] [--open]` | The v2 local web UI: list, edit, fit, build, preview, export, publish, and index (ADR-0002). | Implemented (v2, ADR-0002) |

Exit codes are part of the contract:

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Usage error or a command not implemented yet; not a contract failure code |
| 2 | Invalid spec (unreadable, malformed, schema or validation rule failure) |
| 3 | Geometry or build failure (FreeCAD error, export failure) |
| 4 | Environment error (missing or wrong `freecadcmd`, bad configuration) |

The mapping lives in exactly one place: `src/error.rs`. Commands return `BoxgenError`; only `src/main.rs` converts it to a process exit code.

## Validate

`boxgen validate <spec>` is pure Rust and needs no FreeCAD. It parses the YAML with a closed schema and enforces the generator contract:

- **Schema:** required `system`, `id`, `footprint`, `height_u`, `lid`, and `divisions`; optional `notes`, `contents` (including the `soft` fit-clearance switch), and the `lid_ribs` switch. Unknown fields are rejected at the root, inside every division node, and inside `contents`, so wall thickness, radii, lip dimensions, and print settings cannot be expressed. `lid_ribs` must be a boolean and requires `lid: true`; it defaults to `false` and adds the standard's optional pair of 0.15 mm friction ribs. `system` must be `SYS-1.0`; `id` must be lowercase kebab-case or the standard `<footprint>-<height>U-<slug>-<rev>` form (for example `4x4-6U-usb-cables-v1`); each footprint side is 1–4; and `height_u` is 3, 6, 9, or 12. The standard's uppercase height unit wins over the generator contract's lowercase-only example; arbitrary uppercase is still rejected.
- **Division tree:** each node is a leaf (optional `compartment`) or a split (`axis`, `at_mm`, `near`, `far`). The root covers the body interior, `N × 42 − 4.5` by `M × 42 − 4.5` mm. Dividers are 2.0 mm and `at_mm` must be an integer ≥ 12: the position is the distance from the parent rectangle's origin edge to the divider's near face, and the far child starts 2.0 mm later. Every leaf must be at least 12 mm in both directions; leaves below 15 mm warn. Duplicate compartment names, a path deeper than 32, and a divider coincident with an ancestor divider on the same axis are errors.
- **Contents:** when present, a weight above 10 kg warns. Fit warns when length exceeds the interior diagonal minus 10 mm, width plus the per-side clearance exceeds the interior width, or height plus the vertical clearance exceeds the interior depth. The clearances follow standard §8: 2 mm per side and 5 mm above for rigid items, 5 mm per side and 10 mm above when `contents.soft: true` (soft items and cables). The standard's 10 mm "where fingers must reach" rule stays a human sizing rule, so the schema has no field for it.
- **Estimates:** the report includes a deliberately simple mass and print-time estimate (`shell + dividers + lid`, PETG 1.27 g/cm³, 5.5 mm³/s effective flow). Mass above 500 g or time above 20 h warns. These are estimates, not slicer results.

Warnings never change the exit code. Errors go to stderr with schema paths (for example `divisions.near.far: leaf 9.5 mm wide is below the 12 mm minimum`); semantic errors are collected together where possible. Unreadable and malformed files also exit 2. A valid spec prints its id, footprint and outer dimensions, height, interior dimensions, compartment count and names, estimate, and warnings to stdout, then exits 0.

## Build

`boxgen build <spec> [--out DIR] [--publish]` is the whole compiler: the division tree is compiled into the body (every split is a 2.0 mm divider fused into the shell), `lid: true` builds the standard inset friction lid, and `lid: false` builds the body alone:

1. validate the spec (exit 2 on any schema or rule failure, including an invalid division tree);
2. resolve the output directory: `--out DIR`, or `build/<id>` relative to the current directory;
3. remove the artifacts boxgen owns from an earlier run (`box.py`, `box.step`, `body.stl`, `lid.stl`, `box.FCStd`, `report.json`, `cad/`, `__pycache__/`, and `*.FCBak`), then copy the repository's FreeCAD master into `<out>/cad/` (top-level `*.py` plus `LICENSE-LGPL-2.1`; `cad/tests/`, `__pycache__/`, and `*.pyc` are excluded);
4. render the golden `box.py` and run `freecadcmd <abs box.py>` with the build directory as cwd;
5. verify `report.json`, `box.step`, `body.stl`, `box.FCStd`, and `lid.stl` (only when `lid: true`) exist and are non-empty, then require one valid, closed body solid with positive volume and a watertight STL; a lid spec additionally requires one valid, closed lid solid, a watertight `lid.stl`, and a `lid` report section whose `ribs` flag matches `lid_ribs`;
6. print a human-readable summary to stdout and add the validation warnings to `report.json` (a leaf below 15 mm warns without failing the build);
7. with `--publish`, verify and copy the accepted print files into the spec's box folder (see Publish below).

Artifacts:

```text
build/<id>/
  box.py          generated script; never hand-edited (golden snapshot)
  box.FCStd       review document: Body, plus Lid when lid: true
  box.step        both solids in one STEP (body, plus lid when lid: true)
  body.stl        binary, watertight print mesh
  lid.stl         binary, watertight print mesh (only when lid: true)
  report.json     measured dimensions, divider plan, volume, STL facts, lid facts, estimates, warnings
  cad/            the copied FreeCAD master the script imports
```

`report.json` is deterministic: no timestamps, no wall-clock durations, sorted keys, and the same text for the same spec and master. It carries the measured dimensions, the resolved `dividers` plan (`count`, `height_above_floor_mm`, `top_mm`, and one entry per split with the absolute `near_face_mm`/`far_face_mm` and the full parent `span_lo_mm`/`span_hi_mm` in centred coordinates), the body volume/area/faces and validity, `body.floor_fillet_mm` (the floor-to-wall fillet radius actually applied: 2.0 mm, or the 1.0 mm fallback), the body STL facts, artifact sizes, mass and print-time estimates (body plus lid when present), and warnings. For a lid build it also carries `lid` (volume/area/faces/solids/validity, bbox, `roundover_mm`, and the `ribs` flag) and `lid_stl` (facets, watertightness, volume). `body.stl`, `lid.stl`, and `report.json` are byte-identical across runs; `box.step` differs only in its `FILE_NAME` timestamp; `box.FCStd` is a zip with an internal timestamp (not hashed).

The lid follows standard §4.4: a 2.0 mm plate whose top sits 1.0 mm below the lip top (`height_u * 7 - 1.0`), a 2.0 mm thick x 2.0 mm deep skirt with 0.30 mm per-side clearance to the lip, 0.6 mm skirt-bottom chamfer, 1.40 mm outer plate corners, a 1.0 mm top roundover, and one 20 mm finger hole centred 14 mm from the +y footprint edge. At the design stack pitch the next body's base bottom starts one pitch up (`height_u * 7`), 1.0 mm above the lid top, so the lid is out of the stack load path there. `cad/tests/lid.py` also seats the same base in the lip's *measured contact position* (base bottom 0.35 mm below the lip bottom) and reports the clearances to the lid's plate edge and top face: both are negative (about -1.15 mm and -0.6 mm), i.e. a fully seated base would rest on the lid rather than the lip. That is the documented finding for the human checkpoint (#14); the architecture is unchanged pending that checkpoint. `lid_ribs: true` adds the standard's optional pair of 0.15 mm friction ribs to the skirt and requires `lid: true`; `lid_ribs: false` is the default. The lid is sized per axis, so a 2 x 4 body does not get a square lid.

`freecadcmd` exits 0 even on an unhandled Python exception, a syntax error, or a missing script. The build therefore never trusts the exit status alone: missing, empty, or unparseable artifacts are a build failure (exit 3). A missing launcher or a release other than `BOXGEN_EXPECT_FREECAD_VERSION` is an environment failure (exit 4).

The 1×1×6U body from `tests/fixtures/minimal-build.yaml` measures 41.5 × 41.5 × 42.0 mm, has an interior depth of 33.25 mm, one closed shell, and roughly 18 397.5 mm³ / 23.4 g (before slicer settings). Its outer vertical corners are 3.75 mm, its inner body and lip corners follow `OUTER_RADIUS − offset` (1.75 mm through the body zone, 1.15 mm at the lip neck, 1.85 mm through the lip vertical section), and the floor-to-wall cove is a 2.0 mm radius; no internal corner above the base is sharper than 1.0 mm. The base zone is solid (ADR-0013): the floor is its top face and the bottom prints as filled foot pads, with the sharpest base curve the pinned profile's 0.8 mm bottom chamfer radius. `tests/build.rs` pins the dimensions, the `body.floor_fillet_mm` value, a ±3% volume range, the lid artifact and report contract, byte-level determinism, and the exit-code cases; `tests/body_radii.rs` slices the built body and pins every radius. The 2×4×6U lid fixtures (`tests/fixtures/2x4-6u-lid.yaml` and `2x4-6u-lid-ribs.yaml`) pin the lid build end to end, while `cad/tests/lid.py` + `tests/lid.rs` slice and measure the lid's seven standard features and the rib variant. `tests/golden/minimal.box.py` and `tests/golden/2x4-6u-lid.box.py` are byte-for-byte snapshots of the generated scripts; update them only for an intentional script change.

### Publish

`--publish` writes into the spec's own box folder and nowhere else:

- **Target:** the spec file's parent directory, which must be named after the spec `id` (the `boxes/<id>/spec.yaml` layout). A mismatch is a usage error (exit 1) that names the expected layout; boxgen never invents a second location.
- **Verify first:** `box.step`, `body.stl`, `notes.md`, and `lid.stl` (only when the spec has `lid: true`) must exist and be non-empty before the first copy. A missing or empty artifact is a build failure (exit 3) that leaves the target folder untouched — no partial publish.
- **Notes:** when the build directory has no `notes.md`, publish generates one from the spec (id, notes, contents, footprint, height, catalogue stack group, lid, and a blank print record with `Printed:`) and writes it into `build/<id>/notes.md` as well. The box folder keeps its own `notes.md` if one exists: it is the human print record and is never overwritten.
- **Copy:** `box.step`, `body.stl`, `lid.stl` when present, and `notes.md`. `box.FCStd`, `report.json`, `box.py`, and `cad/` are never published, and `spec.yaml` is never rewritten.

Without `--publish`, the build stays in `build/<id>/` and `boxes/` is untouched.

## Coupon

`boxgen coupon [--out DIR]` builds the **calibration coupon** exactly as specified in the spec's `docs/spec/docs/calibration.md`: a 1 × 1, 6U body with the standard architecture (2.0 mm walls and lip; a solid 4.75 mm base zone whose top face is the floor; 3.75/1.75 mm corner radii; 2.0 mm joint coves; 0.4 mm divider top chamfer), one 2.0 mm T-divider fused to the floor and one wall with its top 10 mm below the lip (z = 28.0), and the ribbed friction lid (two 0.15 mm ribs, 20 mm finger hole, 0.30 mm clearance).

The coupon is not a user spec: `boxgen coupon` constructs the one internal spec in code (`coupon-1x1-6U-v1`, `footprint [1, 1]`, `height_u 6`, `lid: true`, `lid_ribs: true`, a single-leaf division tree, and an internal `coupon: true` flag) and runs it through the **normal master/build path** — the same `cad/build.py::build(spec, out_dir)` that builds every body, with the T-divider fused by `cad/dividers.py::build_coupon_divider`, no hand-made geometry. That is the point: passing the physical tests validates the real pipeline. The closed user schema still rejects `coupon` as an unknown field (exit 2), so a hand-written spec can never approximate the coupon.

```text
build/coupon/            the default --out
  box.py                 generated script; imports cad.build like every build
  box.FCStd              review document: Body and Lid
  box.step               both solids in one STEP
  body.stl               watertight body mesh (with the T-divider)
  lid.stl                watertight ribbed lid mesh
  report.json            the usual report plus the `coupon` section
  cad/                   the copied FreeCAD master
```

The `coupon` report section records the T-divider (`wall: -y`, 20 mm from the wall's inner face, 2.0 mm thick, 28.0 mm top, 0.4 mm chamfer); the Rust build requires it for the coupon spec, so the master cannot silently skip the divider and still pass.

After a successful build the command prints the calibration reminder: **print two coupons and one lid** in the production recipe, run the nine tests in `docs/spec/docs/calibration.md`, and record the result in its record table. The coupon is the only iteration; no production box is printed until it passes every acceptance criterion.

`scripts/calibration-wizard.sh` walks the human checkpoint (ticket #13): it checks the coupon build, steps through the Bambu Studio/X2D production recipe and the print, walks the nine tests one stage at a time, resumes across the 1 h and 24 h load-test waits (state in `build/calibration/state.env`), and writes a dated record with the go/no-go, the calibration-record-table row, and a paste-ready issue comment. The calibration record table stays in the spec (`docs/spec/docs/calibration.md` §5): the wizard prints the row to paste there, and closing #13 or #14 stays a human action — the wizard prints the `gh` commands.

## Index

`boxgen index [--boxes DIR]` (default `boxes`) regenerates `boxes/INDEX.md` from the registry and needs no FreeCAD:

1. scan `<DIR>/*/spec.yaml`, one box folder per ID (other files and folders are ignored);
2. parse and validate every spec with the same rules as `boxgen validate` — a malformed or invalid spec exits 2 and names the file and the rule;
3. render the table deterministically, sorted by box ID, with `\n` line endings and no timestamps;
4. write through a temporary file and an atomic rename, so a failure cannot truncate an existing index.

| Column | Value |
| --- | --- |
| Box ID | The spec `id`. |
| Footprint | `<N> × <M>` in spec order, using U+00D7. |
| Height | `<U>U`, for example `6U`. |
| Contents | `contents.description`, else `notes`, else `—`; `\|` is escaped and newlines collapse to spaces. |
| Stack group | Catalogue group by the sorted footprint pair, so orientation does not matter: 4 × 4 = A, 3 × 4 = B, 2 × 4 = C, 2 × 3 = D, 1 × 4 = E, every smaller or other footprint = F. |
| Printed | The print date from `boxes/<id>/notes.md`, read from a line starting `Printed:`, `Print date:`, or `Printed on:`; `—` when there is no record. |

An empty registry keeps the committed header exactly: the sentence "Generated by `boxgen index`. No boxes recorded yet." and a single `—` row. With boxes, the sentence is "Generated by `boxgen index`." and one row follows per box. `tests/fixtures/committed-INDEX.md` is a copy of the spec's committed empty index; the test suite compares against it byte for byte and, when the vendored `docs/spec/` is present, against `docs/spec/boxes/INDEX.md` as well.

## Divide

`boxgen divide <spec> --path <p> --axis <x|y> --at <mm>` edits one node of a spec's division tree in place, then re-parses and re-validates the whole spec before writing. The edit is lossless: only the target node's text changes, so the rest of the file keeps its comments and formatting.

**Path syntax.** A path is `root`, or starts at `root` and follows `.near` / `.far` steps:

```text
root                 the root division node (also the default when --path is omitted)
root.near            the near child of the root
root.far.near        the near child of the root's far child
```

**Editing a split.** `--axis` and `--at` set the split's axis and near-face position. If only one flag is given, the other field keeps its current value. If neither is given, the command is a usage error (exit 1) rather than a silent no-op.

**Editing a leaf.** A leaf has no split fields to update, so both `--axis` and `--at` are required; the leaf is replaced by a split with two leaves. If the original leaf had a `compartment` name, that name moves to the near child and the far child is unnamed. An unnamed leaf produces two unnamed children.

**Errors and formatting.** An unknown or unresolved path (for example `root.middle`, or `root.near.far` when `root.near` is a leaf) exits 2. The edited spec is validated with the same rules as `boxgen validate`; an invalid result (a leaf below 12 mm, a coincident divider, a tree deeper than 32, and so on) exits 2 with the rule and schema path. Every failure leaves the file byte-for-byte unchanged. YAML comments and indentation outside the target node survive; comments *inside* a leaf that is replaced by a split may be dropped with that leaf.

```bash
boxgen divide boxes/example/spec.yaml --path root.near --axis x --at 40
boxgen divide boxes/example/spec.yaml --at 60          # edit the root split's position only
boxgen divide boxes/example/spec.yaml --axis y --at 18 # turn the root leaf into a split
```

## Web UI

`boxgen serve [--port PORT] [--open]` (v2, ADR-0002) is the local web UI over the same spec. It binds `127.0.0.1` only — one local user, no auth — and serves the embedded frontend (vanilla ES modules and SVG; no Node, no CDN) plus the API. The workspace is the process working directory: `boxes/`, `build/`, and `exports/` resolve under it, and `BOXGEN_UI_HOST_WORKDIR` overrides the host path shown for displayed paths.

Start it in Docker with one command (works without `just`):

```bash
just ui                # or scripts/boxgen-ui.sh
just ui --port 8799    # a different port
```

The script ensures both pinned images (`boxgen/freecad:1.0.0` and `boxgen/ui:1.0.0`), mounts the current directory at `/app`, runs the container as your UID, prints the URL (default `http://127.0.0.1:8787/`; `--port` changes it), and opens the browser; Ctrl-C stops the server. On Linux it uses `--network host`, because `boxgen serve` binds the container's loopback and a published port would forward to the bridge address instead.

The UI does: list the registry, open/edit/save-as boxes, split and drag dividers with live validation (every edit is planned by the Rust validator through `/api/plan`; the JS never implements a rule), the fit assistant, a read-only system standard panel, and the one build pipeline — one build at a time — with 3D preview of the real meshes, named exports into `exports/<base>/`, publish, and `boxgen index`. It does not: expose geometry parameters (walls, radii, and the lip stay fixed and read-only), run in the cloud, or add accounts. Drafts autosave in the browser's localStorage; saving writes the canonical `spec.yaml` (only when the design is valid), so comments are dropped there while `boxgen divide` stays lossless.

Open tolerates a registry folder name that differs from the spec id, but Save and Publish require the standard `boxes/<id>/` layout (Save as creates it); publish follows the CLI rule exactly and never writes into a legacy folder.

`docs/serve-walkthrough.md` is the human browser checklist (ticket #22; no browser automation by decision).

## Configuration and logging

| Variable | Default | Meaning |
| --- | --- | --- |
| `BOXGEN_LOG` | `warn` | Stderr log level: `trace`, `debug`, `info`, `warn`, `error`, or `off`. |
| `BOXGEN_FREECADCMD` | `freecadcmd` | Headless FreeCAD launcher used by `build` and `coupon`. |
| `BOXGEN_EXPECT_FREECAD_VERSION` | `1.0.0` | FreeCAD release `build` requires from `<launcher> --version`. The `justfile` passes its pin here. |
| `BOXGEN_UI_PORT` | `8787` | Port `boxgen serve` binds on `127.0.0.1`; `--port` overrides it. |
| `BOXGEN_UI_HOST_WORKDIR` | unset | Host path shown for displayed workspace paths (the Docker launcher sets it to the mounted directory). |

Typed config is parsed once in `src/config.rs`. Reports go to stdout; diagnostics go to stderr as human-readable `tracing` output, so command output can be piped without JSON noise.

## FreeCAD master (`cad/`)

The FreeCAD Python master lives in `cad/`. `boxgen build` copies the importable
modules into `<out>/cad/` so the generated script is self-contained and
reviewable; `cad/tests/` never enters a build.

- `cad/gridfinity.py` — the pinned Gridfinity base foot and stacking-lip
  profiles, with the public constants (`GRID`, `CLEARANCE`,
  `NOMINAL_WIDTH_1U`, the 0.8 + 1.8 + 2.15 base split, the 0.7 + 1.8 + 1.5
  lip split, the 0.4 mm ledge, and the corner radii), the rounded-rectangle
  helpers, `make_base_foot` / `make_base_feet`, the base underside cutter,
  and the lip cut and ring. It is **vendored from the FreeCAD Gridfinity
  Workbench** (LGPL-2.1, attribution kept in the file header and the license
  text in `cad/LICENSE-LGPL-2.1`), adapted rather than redrawn, and it imports
  only `FreeCAD` and `Part`: no installed workbench is required or called.
- `cad/fillets.py` — the SYS-1.0 internal-fillet contract (ticket #7):
  `fillet_with_fallback(shape, radius, edges, fallback=1.0)` tries the
  standard 2.0 mm radius, accepts it only when the result is valid, contains
  exactly one solid, and has positive volume, then tries the 1.0 mm fallback
  the same way, and raises `GeometryError` when both fail. The reduce-then-fail
  rule is the standard's section 4.1; a sharp internal corner above the base is
  never returned. `makeFillet` can "succeed" with an invalid shape, hence the
  explicit acceptance checks.
- `cad/geometry.py` — the shared shape invariants: `GeometryError` and
  `require_one_solid(shape, what, *, closed=False)`, the one-solid,
  positive-volume (and optionally one-closed-shell) guard that `cad/shell.py`,
  `cad/dividers.py`, and `cad/lid.py` use at every construction step.
  `GeometryError` is re-exported by `cad/fillets.py`; the vendored
  `cad/gridfinity.py` keeps its own standalone guard because it must import
  with only `FreeCAD` and `Part`.
- `cad/shell.py` — the SYS-1.0 body shell for ticket #6: outer prism (3.75 mm
  corners), one centred base foot per cell, the pinned underside cutters
  (solid base zone with the floor as its top face, ADR-0013), the
  pre-filleted interior cutter (2.0 mm floor cove through `cad/fillets.py`,
  1.0 mm fallback), a cell-boundary web per internal 42 mm grid line (fills
  each 0.5 mm Gridfinity groove over the 0.2 mm lift band, so the interior
  feet of a 4 × 4 body stay fused to the prism instead of coming out as
  separate solids; it sits above the base zone where a standard baseplate's
  rib has already narrowed to zero width), and the pinned lip void;
  `build_body` returns a `BodyResult` with the body and the fillet radius
  actually applied. Every fuse and cut must leave exactly one solid with
  positive volume; `GeometryError` otherwise.
- `cad/dividers.py` — the division tree compiled to solids (ticket #8):
  `divider_plan` walks the tree from the body interior origin and resolves
  each split to its absolute rectangle (centred coordinates), `divider_height`
  is interior depth minus 2.0 mm on 3U and minus 10.0 mm on 6U and taller,
  and `build_dividers` fuses each 2.0 mm plate (chamfered 0.4 mm on its top
  edges) plus a 2.0 mm cove rail per face and a vertical cove column per span
  end into the shell, checking exactly one closed solid at the end. OCC
  cannot fillet a fused shell's junctions, so the coves are built as box
  minus quarter cylinder, the verified recipe. A divider face or side that
  lands on the 1.75 mm inner corner arc gets the same column on the projected
  wall face: the 2.0 mm cove is larger than the corner radius and buries the
  acute notch, so no sharp internal corner remains (contract validation makes
  the case unreachable — a split's far child is at least 12 mm deep — but
  `cad/tests/dividers.py` forces it with synthetic trees). For the coupon
  (ticket #10) `build_coupon_divider` fuses the calibration T-divider: one
  2.0 mm plate centred on the interior's x = 0, running 20.0 mm from the -y
  wall's inner face into the interior (the calibration's 20 mm weight foot
  rests on the 20 mm top), top at 28.0 mm, the 0.4 mm top-edge chamfer, and
  the same floor rails and wall cove columns as a tree divider; the free end
  carries no column.
- `cad/lid.py` — the SYS-1.0 inset friction lid (ticket #9):
  `build_lid(units_x, units_y, height_u, ribs=False)` returns one valid closed
  solid. The plate is `outer_width(units) - 2 * (1.9 + 0.30)` per axis, so it
  clears the lip's vertical section by 0.30 mm per side; the skirt is a stack
  of monotonic loft segments following `gridfinity.lip_inner_offset` with the
  same clearance, because one multi-row loft through the non-monotonic lip
  profile self-intersects under OCCT 7.8. Edges are the 0.6 mm skirt-bottom
  chamfer and the 1.0 mm plate-top roundover (optional at the 2.0 mm plate
  limit; `top_roundover_applied` reports the outcome). The 20 mm finger hole
  is centred 14 mm from the +y footprint edge. `ribs=True` fuses the optional
  0.15 mm x 10 mm x 1.5 mm interference ribs on the +x and -x skirt faces;
  the visible rib stays inside the plate footprint.
- `cad/build.py` — `build(spec, out_dir) -> dict`. Builds the body (shell
  plus the division tree's dividers, or the calibration T-divider when the
  internal `coupon: true` flag is set) and, for `lid: true`, the lid, creates
  `Part::Feature` objects named `Body` and `Lid` in one document, saves
  `box.FCStd`, exports `box.step` with
  `Import.export([body_object, lid_object], ...)`, meshes `body.stl` (and
  `lid.stl` when present) with `MeshPart.meshFromShape(...)`, and returns the
  report (measured dimensions, interior depth, the resolved `dividers` plan,
  body/lid/STL facts including `body.floor_fillet_mm`, `lid.roundover_mm`,
  and `lid.ribs`, the `coupon` T-divider section for the coupon, artifact
  sizes, mass and print estimates for body plus lid, warnings; a reduced
  fillet or a rejected roundover is a warning). A body-only spec writes no
  `lid.stl` and clears a stale one. The coupon flag is fixed to the
  calibration's 1 × 1 × 6U ribbed geometry: anything else is a
  `GeometryError`.
- `cad/__init__.py` — makes `from cad.build import build` (and the generated
  script's `import cad.build`) work from the build directory.
- `cad/tests/_harness.py` — the shared scaffolding of the headless
  measurement scripts: the `BOXGEN_JSON:` marker and its flush, the
  six-decimal rounding, the thin-slab section measurement, the material
  probes and bisections, and the launcher-safe entry-point wrapper. Every
  script imports it as `cad.tests._harness`.
- `cad/tests/interface.py` — the headless interface measurement script. It
  imports the module, builds the base foot, the lip cut, and
  the lip ring, measures them at the profile levels, and prints one
  `BOXGEN_JSON:` line.
- `cad/tests/radii.py` — the headless radii measurement script (ticket #7). It
  exercises the fillet helper on the smallest legal interior and on synthetic
  thin prisms, slices the built 1x1x6U and 1x1x3U bodies, and prints one
  `BOXGEN_JSON:` line.
- `cad/tests/lid.py` — the headless lid measurement script (ticket #9). It
  builds the non-square 2×4×6U lid plain and ribbed plus the matching body,
  slices the plate, skirt, pilot, and chamfer, collects the corner, roundover,
  and finger-hole circles, and prints one `BOXGEN_JSON:` line.
- `cad/tests/dividers.py` — the headless divider measurement script
  (ticket #8). It resolves the plan for the contract's golden tree, builds
  4 × 4, 1 × 1, and two synthetic corner-arc trees, slices them above the
  floor, at the divider tops, and inside the 0.4 mm chamfer band, probes the
  T, wall, and corner junctions, measures every legal divider top by
  bisection, and prints one `BOXGEN_JSON:` line.
- `cad/tests/coupon.py` — the headless coupon measurement script (ticket
  #10). It builds the coupon body (`cad.shell.build_body` +
  `cad.dividers.build_coupon_divider`) and the ribbed 1 × 1 × 6U lid,
  measures the T-divider's faces, top, length, and chamfer by bisection, the
  thin-slab slice areas at z = 20 and z = 29, the 2.0 mm floor and wall
  coves by material probes, and the lid's plate, finger hole, pilot
  clearance, and ribs, and prints one `BOXGEN_JSON:` line.
- `cad/tests/step_slices.py` — the headless STEP slice script (ticket #8).
  It loads an exported `box.step` under `freecadcmd`, finds the `Body`
  product, and reports the bounding box of every section wire at the heights
  in `BOXGEN_SLICES` plus the area each wire encloses; `tests/build.rs` uses
  the wire boxes to verify the dividers in the exported model rather than
  only in the report, and `tests/coupon.rs` uses the areas to prove the
  coupon T-divider is in the artifact.
- `cad/tests/standard_mirror.py` — the fixed-architecture mirror script. It
  imports the master modules and emits every constant the Rust side mirrors
  (`src/tree.rs` and the `GET /api/standard` panel), so
  `tests/standard_mirror.rs` can prove the Rust constants and their Python
  counterparts still agree.
- `tests/gridfinity_interface.rs` — runs that script with `freecadcmd` when
  `BOXGEN_REQUIRE_FREECAD=1` and asserts the SYS-1.0 numbers: grid 42 and
  0.25 mm per-side clearance; base 4.75 mm (35.6 -> 37.2 -> 37.2 -> 41.5,
  radii 0.8 / 1.6 / 3.75); lip 4.0 mm (0.7 + 1.8 + 1.5, 0.4 mm ledge, 2.4 mm
  skirt overlap); a 1-unit foot and lip at 41.5 mm; a 4 x 4 base at
  167.5 x 167.5 mm; and the 0.25 mm per-side base-to-lip stacking fit.
- `tests/body_radii.rs` — runs `cad/tests/radii.py` under the pinned FreeCAD
  and asserts the measured radii: outer 3.75 mm, inner 1.75 mm, lip rows at
  `OUTER_RADIUS - offset`, the floor cove at 2.0 mm, every circular edge above
  the base at least 1.0 mm, the base zone's 0.8 mm profile minimum, and the fillet
  helper's reduce and failure paths on synthetic prisms (no legal spec forces
  them).
- `tests/lid.rs` — runs `cad/tests/lid.py` under the pinned FreeCAD and pins
  the lid's standard numbers: 2.0 mm plate, 2.0 x 2.0 mm skirt, 0.30 mm
  per-side clearance at the neck and vertical section, 1.40 mm outer corners,
  1.0 mm top roundover, 0.6 mm bottom chamfer, the 20 mm finger hole at 14 mm
  from the +y edge, the 1.0 mm stack gap, the square 1×1 parity pins, and the
  optional pair of 0.15 mm ribs (measured as exactly 0.30 mm of extra pilot
  width and 3.0 mm² of extra section area).
- `tests/dividers.rs` — runs `cad/tests/dividers.py` under the pinned FreeCAD
  and pins the divider contract: 2.0 mm thickness, the golden tree's absolute
  near faces at x = -1.75, y = -31.75, x = 40.25 with their full spans and a
  single valid closed body, top heights 15.0 / 28.0 / 49.0 / 70.0, the 0.4 mm
  top chamfer, the 2.0 mm floor cove taper, T/wall junction probes, one split
  of a 4 × 4 body staying one solid (the shell's cell webs), and two synthetic
  corner-arc trees whose acute notch is buried by the 2.0 mm cove.
- `tests/standard_mirror.rs` — runs `cad/tests/standard_mirror.py` under the
  pinned FreeCAD and asserts every mirrored `src/tree.rs` constant (walls,
  base, radii, dividers, lid, and the estimate inputs) equals its Python
  counterpart in `cad/*.py`, so the `/api/standard` panel cannot drift from
  the geometry the generated scripts build.

The profiles are the interface that must never drift. Change them only by
bumping the SYS-1.0 standard and recording an ADR; the workbench (and the
`lip.ring.volume` reference in the interface test) is the reference.

## SYS-1.0 specification

The authoritative spec is **vendored in this repository at `docs/spec/`** (it was a separate local repository before 2026-10-18). The generator contract is this crate's blueprint.

| Document | Path |
| --- | --- |
| Generator contract | `docs/spec/docs/generator.md` |
| Design standard (wins on conflict) | `docs/spec/docs/standard.md` |
| Workflow | `docs/spec/docs/workflow.md` |
| Calibration procedure | `docs/spec/docs/calibration.md` |
| Box spec template | `docs/spec/templates/box-spec.yaml` |
| Box registry | `docs/spec/boxes/` |

Key contract points:

- A spec fixes the system version (`SYS-1.0`), box ID, footprint (1 × 1 to 4 × 4 units), height (3U, 6U, 9U, or 12U), lid flag, notes, and the recursive division tree. Wall thickness, radii, lip dimensions, and print settings are fixed by the standard and cannot appear in a spec.
- The division tree is the only stored compartment form. Dividers are 2.0 mm, leaves are at least 12 mm (warn below 15 mm), and nested splits resolve from the parent's origin. Every split becomes a divider spanning its whole parent rectangle, fused into the body with the standard 2.0 mm junction fillets and a 0.4 mm top-edge chamfer; the height above the floor is interior depth minus 2.0 mm on 3U bodies and minus 10.0 mm on 6U and taller (tops at 15.0 / 28.0 / 49.0 / 70.0 mm). Validation warnings (a leaf below 15 mm) flow into the build's `report.json` without failing it.
- `build` writes `build/<id>/box.py`, `box.FCStd`, `box.step`, `body.stl`, `lid.stl` (when `lid: true`), and `report.json`. Only `--publish` copies accepted artifacts (`box.step`, the STLs, `notes.md`) into `boxes/<id>/`; `spec.yaml` is never rewritten and an existing `notes.md` is never overwritten.
- `boxes/INDEX.md` is generated by `boxgen index` from the box folders and is never edited by hand.
- FreeCAD master modules in `cad/` expose `build(spec, out_dir) -> dict`; the Rust side only orchestrates. `cad/gridfinity.py` owns the pinned Gridfinity profiles and is LGPL-2.1, not MIT.

The standard wins if it and the generator contract disagree.

## Repository layout

```
src/
  main.rs        CLI entry: parse, configure, log, map errors to exit codes
  cli.rs         clap definitions, help text, and command dispatch
  spec.rs        closed YAML schema, parsing, validation, validate report
  tree.rs        division tree, body geometry, leaf sizes, estimate, in-place divide
  build.rs       build orchestration: render box.py, copy cad/, run freecadcmd, verify artifacts, publish
  index.rs       boxes/INDEX.md generation from the box specs
  serve/         the local web UI (v2, ADR-0002): router, API, jobs,
                 embedded assets, registry store
  config.rs      typed BOXGEN_* configuration
  logging.rs     stderr tracing subscriber
  error.rs       the one error contract and exit-code mapping
cad/             FreeCAD Python master: gridfinity.py pins the base/lip,
                 geometry.py owns the shared one-solid guard, fillets.py
                 applies the 2.0 -> 1.0 mm fallback rule, shell.py builds the
                 body, dividers.py compiles the division tree (and the coupon
                 T-divider), lid.py builds the friction lid, build.py exports
                 and reports
  tests/         headless interface, radii, divider, lid, coupon, and
                 STEP-slice measurements (freecadcmd), sharing _harness.py
docs/spec/       the vendored SYS-1.0 spec: generator contract, design
                 standard, calibration procedure, ADRs, box-spec template,
                 empty registry
templates/       box-spec.yaml, the copyable spec skeleton
assets/          embedded frontend (ES modules + SVG; no Node, no CDN) and
                 vendored three.js (MIT)
scripts/         calibration-wizard.sh (the human #13 walkthrough) and
                 boxgen-ui.sh (the one-command local UI launcher)
tests/           CLI and integration tests (shared scaffolding in
                 tests/common/, golden scripts under tests/golden/)
docker/          pinned FreeCAD toolchain and gate image; the local UI image
```

## Gates and tests

`just check` is the fast native gate and needs no infrastructure: `fmt-check`,
`lint` (clippy with `-D warnings`), then `test` (unit, integration, and
doctests). `just check-docker` is the authoritative gate: it builds
`boxgen/freecad:1.0.0` and runs the same `just check` inside it with
`BOXGEN_REQUIRE_FREECAD=1`. CI (`.github/workflows/ci.yml`) builds the same
image and runs the same command on every push and pull request. There is no
database, broker, cache, or compose stack; the one HTTP server is `boxgen
serve`, bound to `127.0.0.1` (ADR-0002).

Current tests pin:

- `boxgen --help` and every subcommand help;
- every contract command name (all five are implemented after ticket #10);
- the coupon build end to end under FreeCAD: exit 0, the full artifact set in `build/coupon` (or `--out`), `report.json` with the `coupon` T-divider section (`wall: -y`, 20.0 mm long, 2.0 mm thick, 28.0 mm top, 0.4 mm chamfer), the ribbed 1 × 1 × 6U lid, the calibration reminder on stdout, the golden `box.py` snapshot, and the calibration-document feature table compared against the vendored `docs/spec/`;
- the coupon geometry under FreeCAD: `cad/tests/coupon.py` pins the 41.5 × 41.5 × 42.0 body, the T-divider's 2.0 mm thickness and 28.0 top by bisection, the 20 mm length from the -y wall, the thin-slab slice areas at z = 20 and z = 29 (the divider material and its absence), the 2.0 mm floor and wall coves, the 0.4 mm top chamfer, and the lid's 20 mm finger hole, 0.30 mm pilot clearance, and 0.15 mm ribs;
- the coupon in the exported artifact: slicing `box.step` at z = 20 and z = 29 shows the divider material in the wire areas, plus the 41.5 mm outer wire and one closed Body solid, with the Lid product present;
- coupon determinism: `box.py`, `body.stl`, `lid.stl`, and `report.json` are byte-identical across runs and `box.step` matches after normalizing its `FILE_NAME` timestamp;
- coupon exit codes: 2 for a user spec containing `coupon` (unknown field), 3 for a FreeCAD failure, 4 for a missing launcher;
- every schema rule, including forbidden fields at the root and inside division nodes and `contents`;
- every division-tree rule: `at_mm`, near/far and leaf minimums, the 15 mm warning, duplicate names, depth, and coincident dividers;
- contents fit/weight and mass/print-time warnings, including that warnings still exit 0;
- process-level `validate` behaviour: report on stdout, diagnostics on stderr, malformed and unreadable files exiting 2, and collected schema errors;
- process-level `divide` behaviour: split edits, one-flag edits, leaf-to-split edits at the root and nested, nested and `root` paths, unresolved paths, missing flags, invalid results, byte-for-byte unchanged files on every failure, comment/formatting preservation, and `validate` passing after a successful edit;
- the golden generated `box.py` for `tests/fixtures/minimal-build.yaml` and `tests/fixtures/2x4-6u-lid.yaml`, compared byte-for-byte (`tests/golden/minimal.box.py` and `tests/golden/2x4-6u-lid.box.py`);
- the body-only build end to end under FreeCAD: 41.5 x 41.5 x 42.0 mm ±0.2, interior depth 33.25 mm, one valid closed solid, a ±3% body-volume range, `body.floor_fillet_mm` at the standard 2.0 mm with no fallback warning, a watertight `body.stl`, a self-contained `cad/` copy, no `lid.stl`, and no `Lid` object in the STEP;
- the split-tree build end to end under FreeCAD: the contract's golden 4 × 4 tree (`4x4-6u-tree.yaml`) exits 0 with the resolved `dividers` plan in `report.json` (near faces at x = -1.75, y = -31.75, x = 40.25, full parent spans), and slicing the *exported* `box.step` at z = 20 shows exactly the four contract compartments while z = 30 shows one open interior;
- the deepest legal corner nesting (`4x4-6u-corner.yaml`) building with the small-leaf warning and its exported compartments at the nested positions, a 4 × 4 split whose divider misses the centre base feet staying one solid, and body + dividers + lid in one document with both STEP products and both print files (`4x4-6u.yaml`);
- split-build warnings: a leaf below 15 mm still builds (exit 0) and the warning reaches `report.json` and stdout;
- the divider geometry under FreeCAD: `cad/tests/dividers.py` pins the 2.0 mm thickness, the 0.4 mm top chamfer (measured as a 0.3 mm cut 0.1 mm below the top), the 2.0 mm floor cove taper (1.032 mm clear at 0.25 mm above the floor, flush at the 2.0 mm cove top), the T and wall junction probes, and top heights 15.0 / 28.0 / 49.0 / 70.0 mm for 3U / 6U / 9U / 12U;
- the corner-arc handling under FreeCAD with two synthetic trees whose divider face (and whose divider cross position) lands on the 1.75 mm inner corner arc: the acute notch is buried by the 2.0 mm cove (tip and notch probes material, no section vertex at the sharp tip, the four cove circles at r = 2.0) and the body stays one valid closed solid; contract validation rejects every tree that could reach the arc (`invalid/corner-divider.yaml`, exit 2);
- the lid build end to end under FreeCAD: `body.stl` and `lid.stl`, one STEP with both `Body` and `Lid` products, a ±3% lid-volume range, `lid.roundover_mm` at 1.0, `lid.ribs` false/true matching the spec, a watertight `lid.stl`, and the lid bbox at 79.1 x 163.1 x (37.0..41.0) mm for the 2×4×6U fixture;
- the lid geometry under FreeCAD: `cad/tests/lid.py` slices the built lid and pins the 2.0 mm plate, 2.0 x 2.0 mm skirt, 0.30 mm per-side clearance, 1.40 mm corners, 1.0 mm roundover, 0.6 mm chamfer, 20 mm finger hole at 14 mm, the 1.0 mm stack gap at the design pitch, the seated-base clearances at the lip's contact seat (negative, the checkpoint #14 finding), and the two 0.15 mm ribs;
- the body radii under FreeCAD: `cad/tests/radii.py` slices the built 1x1x6U and 1x1x3U bodies and pins the outer 3.75 mm and inner 1.75 mm corner arcs, the lip rows at `OUTER_RADIUS - offset`, the 2.0 mm floor cove, the 1.0 mm minimum for every circular edge above the base, the base zone's 0.8 mm profile minimum, and the fillet helper's 2.0 → 1.0 mm reduce path and total-failure path on synthetic thin prisms;
- the fixed-architecture mirror under FreeCAD: `cad/tests/standard_mirror.py` pins every mirrored `src/tree.rs` constant against its `cad/*.py` counterpart, so the `/api/standard` panel cannot drift from the geometry the generated scripts build;
- build determinism: `box.py`, `body.stl`, `lid.stl`, and `report.json` are byte-identical across runs (body-only, lid, and split-tree builds), and `box.step` matches after normalizing its `FILE_NAME` timestamp;
- build exit codes: 2 for an invalid spec (including `lid_ribs: true` with `lid: false` and every tree that would land a divider on the corner arc), 3 for a FreeCAD failure or missing/empty/unparseable artifacts, 4 for a missing launcher or wrong version, and 1 for a publish target outside the `boxes/<id>/` layout;
- publish: the STEP/STL/notes copy, a generated `notes.md` when absent, a handwritten `notes.md` preserved, missing or empty artifacts (including `lid.stl` for a lid spec) failing without touching the target, `spec.yaml` unchanged, and `build` without `--publish` leaving `boxes/` untouched;
- `index`: the committed empty format byte for byte, deterministic rows sorted by box ID, `|`/newline escaping, the catalogue stack-group mapping in both orientations, each print-date prefix, missing `notes.md` as `—`, invalid and malformed specs exiting 2, and a blocked write leaving the existing index untouched;
- the committed spec-repository `boxes/INDEX.md` reproduced byte for byte, through the vendored `docs/spec/` and the fixture used when it is absent;
- the serve API in process (`tests/serve.rs`, `tests/serve_build.rs`): embedded assets, the standard panel, canonical save/open, the golden 4 × 4 plan, the fit assistant, the one-at-a-time build job, artifact bytes, export, publish, index, and the end-to-end UI build under FreeCAD;
- the 0/2/3/4 exit-code mapping in `src/error.rs`;
- typed config parsing and the committed YAML template end to end;
- the pinned FreeCAD release: `freecadcmd --version` must report the version
  from `freecad_version` when `BOXGEN_REQUIRE_FREECAD=1`;
- the pinned Gridfinity interface: `cad/gridfinity.py` imports with no
  workbench installed, and its base foot and lip profiles measure against the
  SYS-1.0 numbers under `freecadcmd`.

Native `cargo test` works without FreeCAD: the toolchain and interface tests
print a skip and pass. Only the image can prove the geometry toolchain claim.

## Documentation

- `AGENTS.md` — commands, rules, crate map, and pitfalls for agents working here.
- `GLOSSARY.md` — repository vocabulary; the spec's glossary (`docs/spec/GLOSSARY.md`) stays authoritative for domain terms.
- `docs/adr/` — architecture decisions for this crate.
- `docs/agents/` — issue tracker, triage labels, and domain-doc conventions.
- `docs/serve-walkthrough.md` — the human browser checklist for the local UI (ticket #22).
- `docs/spec/` — the vendored SYS-1.0 spec (generator contract, design standard, calibration, ADRs, template, registry).

## License

MIT — see `LICENSE`. The vendored Gridfinity profile module
`cad/gridfinity.py` is **LGPL-2.1-or-later** (adapted from the FreeCAD
Gridfinity Workbench; see the file header and `cad/LICENSE-LGPL-2.1`). The
embedded frontend vendors three.js 0.185.0 (**MIT**) in `assets/vendor/`;
source URLs and sha256 hashes are in `assets/vendor/LICENSE-three.md`.
