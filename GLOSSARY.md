# boxgen

Vocabulary for the SYS-2.0 spec compiler. The authoritative domain glossary — body, lid, divider, compartment, footprint, height, coupon — lives at `docs/spec/GLOSSARY.md` (the SYS-2.0 spec is vendored at `docs/spec/`); use its terms and spellings.

## Repository concepts

**Spec compiler**:
`boxgen` itself: it validates a YAML spec, renders a FreeCAD Python build script, runs it headless, and records the result. It is not a CAD kernel.
_Avoid_: generator app, box maker, CAD tool.

**Spec**:
The YAML file for one body: system version, box ID, footprint, height, lid flag, the optional `lid_ribs` switch, notes, the optional contents record (with the `soft` rigid/soft fit-clearance switch), and the division tree. The source of truth for that body.
_Avoid_: config, parameters, design file.

**Master**:
The FreeCAD Python modules under `cad/` that implement the fixed architecture and pinned Gridfinity profiles. `build(spec, out_dir) -> dict` is its entry point.
_Avoid_: template, FreeCAD script (that is the generated `box.py`).

**Build directory**:
`build/<id>/`: generated `box.py`, `box.FCStd`, exports, and `report.json`. Never hand-edited.
_Avoid_: output folder, artifacts (be specific).

**Coupon**:
The calibration build: a 1 × 1 × 6U body with one 2.0 mm T-divider fused to the floor and one wall, plus the ribbed friction lid, built by `boxgen coupon` into `build/coupon` through the same master path as every body. It is an internal spec (`coupon: true`), never a user spec. SYS-2.0 is approved (2026-10-20); the coupon remains the only iteration when a re-calibration is needed (spec: `docs/spec/docs/calibration.md`).
_Avoid_: test box, sample, prototype.

**Publish**:
Copying accepted `box.step`, `body.stl`, `lid.stl`, and `notes.md` into `boxes/<id>/`. Only `build --publish` does it; a successful build alone leaves the registry untouched. It verifies every required artifact before the first copy, never rewrites `spec.yaml`, and never overwrites an existing `notes.md`.
_Avoid_: deploy, release, export (export is the FreeCAD step).

**Registry index**:
`boxes/INDEX.md`: the generated table of every box with its footprint, height, contents, stack group, and print date. `boxgen index` regenerates it from the box specs; it is never hand-edited.
_Avoid_: catalog, listing, manifest.

**Contract command**:
One of the five commands fixed by `docs/spec/docs/generator.md` §5: `validate`, `build`, `divide`, `index`, `coupon`. Their names and argument shapes are the public interface; all five are implemented after ticket #10.
_Avoid_: subcommand (fine in clap context, but "contract command" in docs).

**Exit code**:
The process result fixed by the generator contract: 0 success, 2 invalid spec, 3 geometry/build failure, 4 environment. 1 covers usage errors and unimplemented bootstrap commands; it is not a contract failure code.
_Avoid_: error code, status (use exit code for the process result).

**Stub**:
A contract command that parses and validates its arguments but returns `BoxgenError::NotImplemented` (exit 1) until its feature ticket lands. Stubs keep `--help` and the command surface stable. No stub remains after ticket #10; the concept and the error variant stay for future contract commands.
_Avoid_: placeholder, fake, mock.

**Divide**:
The `boxgen divide` helper: edit one split in a spec's division tree in place, addressed by a `root`/`.near`/`.far` path, then re-validate before writing. A leaf target becomes a split; a named leaf keeps its compartment on the near child.
_Avoid_: split command, tree editor.

**Even divide**:
One compartment replaced in the Serve editor by N+1 compartments of equal usable width along one axis, started from a wall side. The near (origin) strip keeps the original compartment name.
_Avoid_: divide by, multi-split, quick divide.

**Wall side**:
A compartment edge that lies on the body's outer wall rather than on a divider. Only a wall side starts an even divide.
_Avoid_: outer edge, border, boundary.

**Serve**:
The `boxgen serve` command: the one local web UI (v2, ADR-0002) over the same terms as the CLI — Spec, Division tree, Body, Compartment, Lid. The API's `design` field is the wire name for a Spec (the ticket #16 name for the same object, not a separate concept). It binds `127.0.0.1` only (one local user, no auth), serves the embedded frontend, and answers every edit through the Rust validator and every build through the one build pipeline: it owns no geometry and no rules.
_Avoid_: web app, frontend, dashboard.

**Standard preset**:
The UI editor's fixed starting design: 4 × 4 · 6U · lid · ribs. The SYS-1.0 calibration is historical; SYS-2.0 is approved as the default (2026-10-20), with its nine calibration tests accepted as complete. Every legal deviation (3/9/12U, another footprint, lid or ribs off) shows an amber "custom" badge with a diff; red is reserved for invalid, and resetting to the preset is always one click.
_Avoid_: default box, sample, template.
