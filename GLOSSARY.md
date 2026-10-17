# boxgen

Vocabulary for the SYS-1.0 spec compiler. The authoritative domain glossary — body, lid, divider, compartment, footprint, height, coupon — lives in the spec repository (`../research/3d-printed-stackable-portable-storage/GLOSSARY.md`); use its terms and spellings.

## Repository concepts

**Spec compiler**:
`boxgen` itself: it validates a YAML spec, renders a FreeCAD Python build script, runs it headless, and records the result. It is not a CAD kernel.
_Avoid_: generator app, box maker, CAD tool.

**Spec**:
The YAML file for one body: system version, box ID, footprint, height, lid flag, notes, optional contents, and the division tree. The source of truth for that body.
_Avoid_: config, parameters, design file.

**Master**:
The FreeCAD Python modules under `cad/` that implement the fixed architecture and pinned Gridfinity profiles. `build(spec, out_dir) -> dict` is its entry point.
_Avoid_: template, FreeCAD script (that is the generated `box.py`).

**Build directory**:
`build/<id>/`: generated `box.py`, `box.FCStd`, exports, and `report.json`. Never hand-edited.
_Avoid_: output folder, artifacts (be specific).

**Publish**:
Copying accepted `box.step`, `body.stl`, `lid.stl`, and `notes.md` into `boxes/<id>/`. Only `build --publish` does it; a successful build alone leaves the registry untouched.
_Avoid_: deploy, release, export (export is the FreeCAD step).

**Contract command**:
One of the five commands fixed by `docs/generator.md` §5: `validate`, `build`, `divide`, `index`, `coupon`. Their names and argument shapes are public even while stubbed.
_Avoid_: subcommand (fine in clap context, but "contract command" in docs).

**Exit code**:
The process result fixed by the generator contract: 0 success, 2 invalid spec, 3 geometry/build failure, 4 environment. 1 covers usage errors and unimplemented bootstrap commands; it is not a contract failure code.
_Avoid_: error code, status (use exit code for the process result).

**Stub**:
A contract command that parses and validates its arguments but returns `BoxgenError::NotImplemented` (exit 1) until its feature ticket lands. Stubs keep `--help` and the command surface stable.
_Avoid_: placeholder, fake, mock.
