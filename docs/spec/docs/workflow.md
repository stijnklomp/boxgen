# Design Workflow

**Status:** `SYS-2.0` (frozen). FreeCAD Python output, YAML specs plus a Python master, CLI first.

This document describes how to go from an item in a drawer to a finished, recorded box. It describes design patterns and interfaces, not a CAD or slicer tutorial.

## 1. Software roles

| Tool | Role | Not its role |
| --- | --- | --- |
| FreeCAD + the pinned Gridfinity profile functions | Geometry engine: base, lip, shell, dividers, lid. Exports STEP and STL. | Box registry, division editor |
| `boxgen` (Rust CLI, planned) | Parse and validate specs, generate FreeCAD Python build scripts, run `freecadcmd`, regenerate the index. | Geometry kernel |
| Bambu Studio | Slice the exported model with the standard print recipe. | Design decisions |
| MakerWorld | Inspiration and accessories. | Part of the standard |
| Bento3D | UX reference for the division editor. Proprietary; not used as the source of truth. | Source of truth |

The standard must remain usable with FreeCAD alone. The generator is an accelerator, not a dependency.

## 2. Two paths

**Manual path (always available):** open the FreeCAD master module, set the footprint, height, and division tree, rebuild, export. Slower, but no generator dependency.

**Generated path (planned):** write a YAML spec, run `boxgen build`, get a FreeCAD script and exported STEP/STL. The generator validates the spec against the standard and refuses to build an invalid box.

Both paths produce the same artifacts and follow the same rules.

## 3. Step-by-step

### Step 1 — Measure the item

Record the maximum bounding box, including plugs, handles, and coiled cable. For cables, record the coil diameter and width. Weigh heavy items.

| Item | Length | Width | Height | Weight | Notes |
| --- | --- | --- | --- | --- | --- |
| | | | | | |

### Step 2 — Choose footprint and height

Apply the sizing rules in [`standard.md`](standard.md#8-sizing-rules). Prefer a footprint that tiles a 4 × 4 plate with its neighbours. Use **6U** unless the contents need 3U (flat), 9U (cables), or 12U (bulky).

### Step 3 — Build the division tree

A compartment is a rectangle. Divide it in half, then move the new divider to the position you want; repeat in any child compartment. The spec stores the tree:

- `axis: x` — a vertical divider that moves left/right;
- `axis: y` — a horizontal divider that moves up/down;
- `at_mm` — distance from the parent's origin edge to the near face of the divider, in integer millimetres;
- `near` / `far` — the two child compartments, either leaves or further divisions.

Example for a 4 × 4 box: split vertically at 80 mm, split the left child horizontally at 50 mm, split the right child vertically at 40 mm. The right child is 81.5 mm wide (`163.5 − 80 − 2.0`), so 40 mm is valid.

```yaml
divisions:
  axis: x
  at_mm: 80
  near:
    axis: y
    at_mm: 50
    near: { compartment: A }
    far:  { compartment: B }
  far:
    axis: x
    at_mm: 40
    near: { compartment: C }
    far:  { compartment: D }
```

Rules:
- every leaf at least 12 mm in each direction (the app warns below 15 mm);
- dividers are 2.0 mm thick and are added between the children automatically;
- divider height is the interior depth − 10 mm (minimum 15 mm);
- every divider is connected to the wall through the tree; floating dividers cannot occur.

The v1 CLI may expose a helper such as `boxgen divide <spec> --path root --axis x --at 80`; the v2 web UI is the Bento3D-style editor over the same tree.

### Step 4 — Validate

The spec must pass every rule in [`standard.md`](standard.md):

- footprint 1 × 1 to 4 × 4;
- height 3U, 6U, 9U, or 12U;
- interior depth fits the item plus clearance;
- diagonal rule for long items;
- stack group footprint matches the destination stack;
- contents ≤ 10 kg;
- division tree valid.

The generator reports errors and warnings; the manual path uses the same checklist.

### Step 5 — Generate the FreeCAD model

The master builds four things:

1. **Base zone and lip** — pinned Gridfinity profile functions (vendored from the FreeCAD Gridfinity Workbench, LGPL-2.1 attribution kept).
2. **Shell** — solid base zone (the pinned base profile filled) with a 0.8 mm floor slab on top, 2.0 mm body and lip; corner radii per the standard.
3. **Dividers** — 2.0 mm walls from the division tree, fused to the floor and outer wall with 2.0 mm fillets.
4. **Lid** — 2.0 mm plate, 2.0 mm skirt × 2.0 mm, 0.30 mm clearance, one 20 mm finger hole.

Do not redraw the Gridfinity base or lip. Do not expose wall thicknesses as free parameters. Do not hand-edit generated files.

### Step 6 — Export

Export for every box:

- **STEP** — one archived solid containing body and lid, editable and future-proof.
- **STL** — separate `body.stl` and `lid.stl` print files for Bambu Studio.

The generated `.FCStd` review file lives in `build/<box-id>/`; the YAML spec and the Python master are the sources of truth.

### Step 7 — Print

1. If this is the first box, print the coupon in [`calibration.md`](calibration.md) and pass every criterion.
2. Print one test box in the 4 × 4 footprint and 6U height; check the lid fit and a stack of two.
3. Print the production body and lid with the standard print recipe.
4. Never print a batch before the coupon and test box pass.

### Step 8 — Post-process

Remove the brim, deburr the bottom edge, and test the lid. There are no labels to apply; identification is by `boxes/INDEX.md` and the contents.

### Step 9 — Record and stack

Fill in `boxes/<box-id>/notes.md`: contents, print date, filament spool, coupon result, actual mass and print time. Regenerate `boxes/INDEX.md`. Stack only with bodies of the same footprint, at most three high. **Remove the lid from every body that has another body above it; lids are top-only.** SYS-1.0 legacy bodies (84 mm 12U, 4.0 mm lip) must not be mixed with SYS-2.0 bodies in one height-critical stack.

## 4. Generator architecture

The generator is a local Rust CLI built on the existing Rust template. It is deliberately thin: it never implements a CAD kernel.

```
spec.yaml ──validate──▶ build/<id>/box.py ──freecadcmd──▶ box.step / body.stl / lid.stl
     │                        │
     │                        └── imports cad/shell.py (master module)
     └── data: footprint, height, division tree, lid options
```

| Component | Responsibility |
| --- | --- |
| `spec` | Parse and validate YAML box specs (footprint, height, division tree, lid). |
| `tree` | Division tree: splits, positions, leaf sizes, minimum sizes, divider layout. |
| `cad/` | FreeCAD Python master: pinned profiles, shell, dividers, lid, export. |
| `build` | Generate the box script, invoke `freecadcmd`, collect STEP/STL. |
| `index` | Regenerate `boxes/INDEX.md` from the specs. |

Commands:

```
boxgen validate <spec>     # check against the standard
boxgen build <spec>        # generate the FreeCAD script and export STEP/STL
boxgen divide <spec> ...   # optional helper for editing the division tree
boxgen index               # regenerate boxes/INDEX.md
boxgen coupon              # generate the calibration coupon
boxgen serve               # v2: local web UI with the division editor
```

The Rust template's server, Postgres, Valkey, and RabbitMQ features are unused in v1. The template's layering, typed config, logging, error contract, and testing patterns are reused where they fit; the box registry is YAML files in git.

## 5. Print-once discipline

The architecture is validated once. After that:

- the coupon is the only test print;
- one test box validates the assembly;
- production boxes are printed once with the standard recipe;
- layout changes are new specs, not edits to a printed box;
- architecture changes are system version bumps with a new ADR.

## 6. Quick checklist for a new box

- [ ] Measure the item; record L × W × H and weight.
- [ ] Choose a footprint from 1 × 1 to 4 × 4 that tiles the 4 × 4 module.
- [ ] Choose a height: 6U default, 3U for flat, 9U for cables, 12U for bulky.
- [ ] Draw the division tree; every leaf ≥ 12 mm; 2.0 mm dividers.
- [ ] Validate with `boxgen validate` or the manual checklist.
- [ ] Build the FreeCAD model and export `box.step`, `body.stl`, `lid.stl`.
- [ ] If this is the first box, print the coupon and pass every criterion.
- [ ] Print one test box; check the lid on the top body and a two-box stack with no lid between the bodies.
- [ ] Print the production body and lid with the standard recipe.
- [ ] Record the print and contents in `boxes/<box-id>/notes.md`; regenerate the index.
- [ ] Stack only with the same footprint, at most three high, 10 kg per body.

The full generator contract is in [`generator.md`](generator.md).

## 7. Bento3D verdict

Bento3D is a proprietary web application. The underlying Nodi engine is Apache-2.0 and self-hostable, but Bento3D itself is not published, has no licence for local use, and exports STL rather than a parametric model. It is retained as a UX reference for the division editor; it is not the source of truth. See [ADR-0002](adr/0002-bento3d-rejected.md).
