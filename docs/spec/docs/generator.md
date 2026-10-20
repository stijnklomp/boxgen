# `boxgen` — Generator Contract

**Status:** `SYS-2.0`. This document is the contract between the design standard and the generator implementation. The generator lives in a separate repository, forked from the Rust template and stripped of the server infrastructure it does not need.

## 1. Purpose

`boxgen` turns a YAML box spec into a FreeCAD model and print files. It is a **spec compiler**, not a CAD kernel:

```
spec.yaml ──validate──▶ build/<id>/box.py ──freecadcmd──▶ box.step + body.stl + lid.stl
     │                         │
     │                         └── imports cad/build.py (master module)
     └── footprint, height, division tree, lid
```

FreeCAD is the geometry engine. The generator never generates meshes itself.

## 2. Repository layout

```
boxgen/
  Cargo.toml
  src/
    main.rs          CLI entry point
    spec.rs          YAML schema and parsing
    tree.rs          division tree, leaf sizes, validation
    build.rs         orchestration and freecadcmd invocation
    index.rs         boxes/INDEX.md generation
  cad/
    gridfinity.py    pinned base/lip profile functions (LGPL-2.1 attribution)
    shell.py         solid base zone, shell, lip, corner radii
    dividers.py      division tree to divider solids
    lid.py           friction lid
    build.py         build(spec, out_dir) entry point
  templates/
    box-spec.yaml
  tests/
```

The Rust template's layering, typed config, logging, error contract, and `just` recipes are reused. Postgres, Valkey, RabbitMQ, and the HTTP server are not used in v1; they may return only if a web UI needs them.

## 3. Input schema

```yaml
system: SYS-2.0
id: 4x4-6U-usb-cables-v1
footprint: [4, 4]        # units, 1..4 each
height_u: 6              # 3, 6, 9, or 12
lid: true
notes: "USB-C and power cables"

divisions:
  axis: x                # x = vertical divider (moves left/right)
  at_mm: 80              # integer mm from parent origin to divider near face
  near:
    axis: y              # y = horizontal divider (moves up/down)
    at_mm: 60
    near: { compartment: "USB-C short" }
    far:  { compartment: "USB-C long" }
  far:
    axis: y
    at_mm: 80
    near: { compartment: "Power" }
    far:  { compartment: "Adapters" }
```

Fields:

| Field | Required | Meaning |
| --- | --- | --- |
| `system` | yes | Must be `SYS-2.0` for this contract. |
| `id` | yes | `^[a-z0-9]+(-[a-z0-9]+)*$`; used as the folder name. |
| `footprint` | yes | `[N, M]`, each 1–4. The larger number is not required to be first; the body is generated as given. |
| `height_u` | yes | 3, 6, 9, or 12. Counts the nominal body height to the lip bottom; the official 4.4 mm lip is above it. |
| `lid` | yes | `true` or `false`. |
| `notes` | no | Free text for `notes.md`. |
| `divisions` | yes | Root division node; see section 4. |

The spec must not contain wall thickness, lip dimensions, corner radii, or print settings. Those are fixed by the standard.

## 4. Division tree

A node is one of:

**Leaf**

```yaml
compartment: "A"   # optional; if present, unique among leaves
```

**Split**

```yaml
axis: x | y
at_mm: <integer>
near: <node>
far: <node>
```

Semantics:

- `axis: x` creates a vertical divider; the position is measured along the parent's width. `axis: y` creates a horizontal divider; the position is measured along the parent's height.
- `at_mm` is the distance from the parent's usable origin edge to the **near face** of the divider, in whole millimetres.
- The divider is 2.0 mm thick. The near child occupies `[0, at_mm]`; the far child occupies `[at_mm + 2.0, parent_size]`.
- The parent's usable size excludes the ancestor dividers; the generator resolves absolute positions from the body interior.
- Every divider spans its whole parent rectangle, so it connects to the outer wall through the tree. Floating dividers cannot be expressed.
- Divider height follows the standard: interior depth minus 2.0 mm on 3U bodies, interior depth minus 10 mm on 6U and taller.
- Lid: 2.0 mm plate, 2.0 mm skirt × 2.0 mm, 0.30 mm clearance per side, one 20 mm finger hole.

Validation:

| Rule | Error / warning |
| --- | --- |
| `at_mm` is an integer ≥ 12 | error |
| Near child size `at_mm` ≥ 12 mm | error |
| Far child size `parent_size − at_mm − 2.0` ≥ 12 mm | error |
| Leaf size ≥ 12 mm in both directions | error |
| Leaf size < 15 mm in either direction | warning |
| Duplicate `compartment` IDs | error |
| Tree depth > 32 | error; prevents degenerate geometry |
| Divider at the same position as a parent divider | error |

The app may render the tree as a grid or nested array; the tree is the only stored form.

## 5. CLI contract

```
boxgen validate <spec>          # parse and validate; human-readable report
boxgen build <spec> [--out DIR] [--publish] # validate, write build/<id>/box.py, run freecadcmd, export
boxgen divide <spec> ...        # optional: edit a split in place (axis/at_mm/path)
boxgen index [--boxes DIR]      # regenerate boxes/INDEX.md
boxgen coupon [--out DIR]       # generate the calibration coupon build
boxgen serve                    # v2 only: local web UI
```

Exit codes:

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 2 | Invalid spec (validation error) |
| 3 | Geometry/build failure (FreeCAD error, export failure) |
| 4 | Environment error (freecadcmd missing, wrong version) |

`build` writes:

```
build/<id>/
  box.py          generated script; never hand-edited
  box.FCStd       review document
  box.step        body + lid
  body.stl        print file
  lid.stl         print file (if lid: true)
  report.json     mass/time estimate, dimensions, validation warnings
```

It copies `box.step`, `body.stl`, `lid.stl`, and `notes.md` into `boxes/<id>/` only when `--publish` is passed; otherwise the box folder is populated by an explicit publish step. This keeps generated artifacts out of the source-of-truth folder until the print is accepted.

## 6. FreeCAD master contract

`cad/build.py` exposes:

```python
def build(spec: dict, out_dir: str) -> dict:
    """Build the body and lid, export STEP/STL, return a report."""
```

Rules:

- Use millimetres; origin at the centre of the outer footprint, z = 0 at the bottom.
- Import the pinned profiles from `cad/gridfinity.py`; never call an installed workbench.
- Base zone: solid; the pinned Gridfinity base profile filled to its 4.75 mm top, with a 0.8 mm floor slab above it (ADR-0013, ADR-0014).
- Body, lip, dividers, and lid: 2.0 mm.
- Build the body and lid as separate solids in one document.
- Apply the corner radii from the standard. If a 2.0 mm fillet fails, reduce to 1.0 mm; if that fails, raise a geometry error. Never leave a sharp internal corner above the base; the base zone keeps the pinned profile's own shelf radii.
- Export STEP with both solids; export STL as `body.stl` and `lid.stl`.
- Save `box.FCStd` for review.
- The script must be deterministic: same spec + same master = same geometry.
- The script must run headless under `freecadcmd` with no GUI and no user interaction.

## 7. Validation rules

The generator enforces the standard:

| Rule | Result |
| --- | --- |
| `system == SYS-2.0` | error otherwise |
| `footprint` each 1–4 | error otherwise |
| `height_u` in {3, 6, 9, 12} | error otherwise |
| `lid` boolean | error otherwise |
| Division tree valid | error/warning per section 4 |
| Body fits the X2D | error if outer > 200 mm in any direction (the 4 × 4 maximum is 167.5 mm, so this is a guard) |
| Estimated mass > 500 g | warning |
| Estimated print time > 20 h | warning |
| Optional `contents` weight > 10 kg | warning |

`contents` is optional metadata, not yet part of the required schema:

```yaml
contents:
  description: "USB-C cables"
  max_length_mm: 200
  max_width_mm: 80
  max_height_mm: 25
  weight_kg: 1.2
```

If present, the generator checks the item fits the chosen interior with the standard clearance and warns if the diagonal rule fails.

## 8. Testing plan

- **Unit tests:** spec parsing, tree validation, leaf-size math, error codes.
- **Golden geometry:** build a known 4 × 4 × 6U spec under `freecadcmd`; assert outer dimensions 167.5 × 167.5 × 46.4 ±0.2 mm, watertight STL, and a stable volume range.
- **Interface test:** assert the base profile and the official 4.4 mm lip dimensions match the pinned Gridfinity reference.
- **Golden script:** snapshot the generated `box.py` for a fixed spec; changes require an intentional snapshot update.
- **Coupon test:** generate the coupon and assert its dimensions and feature set.
- **Manual acceptance:** print the coupon, record measurements in the calibration record; this is the only test that proves the print recipe.
- **CI:** run unit and golden geometry tests in a Docker image with FreeCAD installed; keep the pinned FreeCAD version in the image tag.

## 9. Non-goals

- No direct mesh/CSG kernel in Rust.
- No Postgres, accounts, cloud, or multi-user service in v1.
- No slicer automation; the generator stops at STL/STEP. Bambu Studio slices.
- No label generation; labels are out of scope (ADR-0012).
- No architecture parameters exposed to the user; wall thickness, lip, floor, radii, and lid clearance are fixed by the standard. Lids are top-only; the stack pitch is `U × 7 − 0.35 mm`.
- `system` must be `SYS-2.0`. SYS-1.0 specs are legacy and are not accepted by the current generator; keep them as historical records or rebuild from the old commit.

## 10. Relationship to the standard

[`standard.md`](standard.md) defines the design. This document defines the generator interface. If they disagree, the standard wins; fix the generator or bump the system version and record an ADR.
