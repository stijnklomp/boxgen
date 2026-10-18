# Stackable Storage System — Design Standard

**Status:** frozen `SYS-1.0` · 2026-10-17
**Printer:** Bambu Lab X2D, 0.4 mm hardened steel nozzle
**Material:** Bambu Lab PETG Basic
**Scope:** everything that is fixed about the boxes. Per-box data lives in specs, not here.

> Change this document only by bumping the system version and recording an ADR. The division tree is data, not a design change.

## 1. Purpose

A storage system for household equipment — cables, measuring equipment, tools, pens, screwdrivers, bolts — that can be printed to exact sizes, expanded one box at a time, and stacked vertically. The design must survive years of use in an apartment, including metal tools and bolts, without bending, warping, or cracking under normal storage loads.

The system is:
- **Gridfinity-compatible** at the base and stacking lip, so it works with an open ecosystem and is not tied to any company.
- **Local and reproducible**: FreeCAD is the geometry engine; the generator is a local CLI; no website dependency.
- **Maximum-thickness and heavy-duty**: 2.0 mm walls, a solid 4.75 mm base zone, integral compartments; every feature at the maximum the Gridfinity profile allows.
- **Flat-sided**: nothing protrudes beyond the footprint; stacks are stable and the exterior is smooth.

## 2. Fixed and variable

| Fixed (the architecture) | Variable (per-box data) |
| --- | --- |
| Gridfinity base profile | Footprint N × M (1 × 1 to 4 × 4) |
| Gridfinity stacking lip | Height U (3U, 6U, 9U, 12U; default 6U) |
| 2.0 mm walls; solid 4.75 mm base zone | Compartment division tree |
| 2.0 mm integral dividers | Notes and contents record |
| Inset friction lid | |
| PETG print recipe | |
| 10 kg / 3-high duty class | |

The fixed part is the "one strong design". A body printed today must stack and interchange with a body printed in five years.

## 3. Gridfinity interface

Gridfinity is the open standard created by Zack Freedman (MIT licence). Only its base and lip are used; everything above the base is our own design.

| Property | Value |
| --- | --- |
| Grid | 42 × 42 mm |
| Height unit | 7 mm |
| Clearance between parts | 0.25 mm per side |
| Nominal outer width, 1 unit | 41.5 mm |
| Base profile height | 4.75 mm (0.8 bottom chamfer + 1.8 vertical + 2.15 top chamfer) |
| Stacking lip height | 4.0 mm (0.7 bottom chamfer + 1.8 vertical + 1.5 top chamfer) |
| Maximum footprint | 4 × 4 units (167.5 × 167.5 mm) |
| Magnet holes | None |
| Screw holes | None |

The base and lip profiles are pinned in the master and must never be redrawn. See [ADR-0010](adr/0010-pin-gridfinity-profiles.md).

The **4 × 4 module** is the reference unit of the system: any body from 1 × 1 to 4 × 4 tiles a 4 × 4 baseplate, and mixed footprints sit side by side on that plate. See [ADR-0011](adr/0011-4x4-module.md).

## 4. Body architecture

A body is made of four zones plus the lid interface. **Every wall above the base is 2.0 mm, and the base zone is solid: the floor is the top face of the standard Gridfinity base profile, not a separate slab.**

| Zone | Thickness | Notes |
| --- | --- | --- |
| Base zone | Solid, 4.75 mm tall | Standard Gridfinity base profile, filled; sits on baseplates and in other bodies' lips |
| Body zone | 2.0 mm | Vertical wall from the base to the lip; carries the stack load |
| Lip zone | 2.0 mm | Standard stacking lip; accepts the next body's base |
| Floor | Top of the base zone | The interior floor surface at 4.75 mm; the base below it is material, so the bottom prints on the bed |
| Divider | 2.0 mm | Integral, fused to floor and wall |
| Lid | 2.0 mm plate, 2.0 mm skirt | Inset; not in the stack load path |

2.0 mm is the maximum wall thickness that keeps the standard Gridfinity lip fit (about 2.25 mm is the hard limit). The base zone is solid so that every bottom layer prints on the bed: an earlier hidden underside cavity could only be closed by bridging a ~35 mm unsupported ceiling per cell, which sags and strings in PETG (see [ADR-0013](adr/0013-solid-base.md)). A solid base also removes any floor-deflection question: the floor is the top of 4.75 mm of material supported by the whole base grid. See [`docs/research/gridfinity-wall-thickness.md`](research/gridfinity-wall-thickness.md), [ADR-0007](adr/0007-maximum-thickness-shell.md), and [ADR-0013](adr/0013-solid-base.md).

### 4.1 Corner radii

Sharp corners are stress concentrators. Every vertical and internal corner is rounded, and the radii are derived from the 2.0 mm wall so the wall stays uniform around corners.

| Feature | Radius | Notes |
| --- | --- | --- |
| Outer vertical corners | 3.75 mm | Standard Gridfinity outer radius; keeps baseplate fit |
| Inner vertical corners | 1.75 mm | Body and lip zones: `3.75 − 2.0`; keeps the 2.0 mm wall uniform at corners. The solid base zone keeps the pinned base profile's own shelf radii. |
| Floor-to-wall fillet | 2.0 mm | Internal; removes the weakest internal corner |
| Divider-to-floor and divider-to-wall fillets | 2.0 mm | Internal; applied to every divider junction |
| Divider top edge | 0.4 mm chamfer | Edge break; avoids sharp edges and lapping |
| Lid outer vertical corners | 1.40 mm | `1.75 − 0.30 = 1.45`, rounded down to 1.40 for extra corner clearance |
| Lid skirt bottom | 0.6 mm chamfer | Self-alignment into the lip |
| Lid top edge | 1.0 mm roundover | Handling edge |
| Base and lip profiles | Standard | Take the radii from the pinned profile functions; do not override |

No internal corner above the base may be sharper than 1.0 mm. The base zone's profile is pinned by the standard and keeps its own shelf radii (the sharpest is the 0.8 mm bottom chamfer). If a fillet cannot be applied because two radii conflict, reduce the radius until it succeeds; never leave a sharp internal corner above the base. Validate every junction with the coupon.

### 4.2 Height and interior depth

Total height is `U × 7 mm`. Nominal interior depth is:

```
interior_depth = U × 7 − 4.75 (solid base zone; the floor is its top face) − 4.0 (lip) = U × 7 − 8.75 mm
```

| Height | Total height | Nominal interior depth | Use |
| --- | --- | --- | --- |
| 3U | 21.0 mm | 12.25 mm | Flat items: a single layer of bolts, bits, adapters |
| 6U | 42.0 mm | 33.25 mm | Universal default: pens, tools, measuring equipment |
| 9U | 63.0 mm | 54.25 mm | Cables, medium tools, power supplies |
| 12U | 84.0 mm | 75.25 mm | Bulky cables, large tools; shelf or deep drawer only |

The floor is the top face of the solid base zone at 4.75 mm; it is not a slab above the base. The lid skirt intrudes 1.0 mm below the lip bottom; the sizing rules leave 2 mm below the lid, which covers it. The generator reports the exact usable depth from the model; the table is the design figure. The default height is **6U**; see section 9.

### 4.3 Compartment division tree

The division tree defines the compartments as a **recursive structure**, not a uniform grid. A compartment is a rectangle; it can be split along one axis into two child compartments; each child can be split again. This matches the "divide in half, then move the divider" workflow and supports exact, non-uniform compartment sizes.

- **Axis:** `x` is a vertical divider that moves left/right; `y` is a horizontal divider that moves up/down.
- **Position:** the distance in **integer millimetres** from the parent rectangle's origin edge to the **near face** of the new divider. The divider is 2.0 mm thick; the far child starts 2.0 mm later.
- **Minimum leaf:** a compartment must be at least **12 mm** in each direction. The app warns below **15 mm**, which is the practical minimum for cleaning and finger access.
- **Equal split:** "divide in half" sets the position to `(parent_size − 2.0) / 2`, rounded to 1 mm.
- **Connectivity:** every divider spans its whole parent rectangle, so it connects to the outer wall through the tree. Floating dividers cannot occur by construction.
- **Divider height:** for 3U bodies the divider is interior depth minus **2.0 mm** (to clear the lid skirt); for 6U and taller it is interior depth minus **10 mm**. This leaves a finger gap above the dividers while keeping them fused to the wall along their full length. The gap is below the lip, so the lid never presses on the contents.
- **Thickness:** every divider is 2.0 mm, fused to the floor and wall with the 2.0 mm fillets in section 4.1.
- **Canonical form:** the division tree is the spec. A uniform grid or array view is a rendering; the character-grid shorthand may be used only when every split is equal.

Example tree for a 4 × 4 box: split the whole interior with a vertical divider at 80 mm, split the left child horizontally at 50 mm, and split the right child vertically at 40 mm. The right child is 81.5 mm wide (`163.5 − 80 − 2.0`), so 40 mm is a valid split.

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

See [ADR-0008](adr/0008-recursive-division-tree.md).

### 4.4 Lid

- Inset friction lid: plate **2.0 mm**, skirt **2.0 mm thick × 2.0 mm deep**.
- Skirt clearance **0.30 mm per side**; bottom chamfer 0.6 mm for self-alignment.
- Lid top sits **1.0 mm below the lip top**, so it never contacts the body above.
- One **20 mm diameter finger hole**, centred 14 mm from one edge, to lift the lid. The hole is the only opening; the lid is not dust-proof.
- No latch, magnet, hinge, or screw. If a box must be moved, tape or a strap around the body.
- Friction tuning: if the lid is loose, add two 0.15 mm interference ribs on the skirt; if tight, increase clearance to 0.35 mm. Validate with the coupon.
- No labels, embossed text, or colour coding. See [ADR-0012](adr/0012-no-labels.md).

### 4.5 Stacking

- Only bodies with the **same footprint** may be stacked. A smaller body on a larger body's lip is not supported and is forbidden.
- Mixed footprints sit side by side on a 4 × 4 baseplate or shelf; they are not stacked.
- Maximum **3 bodies high**, maximum **10 kg contents per body**.
- The bottom body's wall carries at most 20 kg of contents from the two bodies above; the floor carries at most 10 kg; the lid carries nothing.
- **Nothing protrudes beyond the footprint**: no latches, handles, or lid overhang.
- Stack pitch equals the body height (`U × 7 mm`); the lid adds no pitch.
- Optional 4 × 4 Gridfinity baseplates are for side-by-side placement; they are not required for stacking.

## 5. Material and print recipe

Every body, lid, and coupon uses the same recipe. The recipe is part of the design, not a per-print choice.

| Setting | Value |
| --- | --- |
| Material | Bambu Lab PETG Basic |
| Nozzle | 0.4 mm hardened steel |
| Layer height | 0.20 mm |
| Wall loops | 5 (2.0 mm) |
| Top shells | 5 (1.0 mm) |
| Bottom shells | 5 (1.0 mm) |
| Infill | 40% gyroid |
| Supports | None |
| Brim | 5 mm |
| Build plate | Textured PEI |
| Body orientation | Base on the plate, opening up |
| Lid orientation | Top surface on the plate, skirt up |
| Temperature | Bambu PETG Basic profile (typically 250–260 °C nozzle, 70 °C bed) |
| Chamber heating | Off; do not enable active chamber heating for PETG Basic |
| Drying | 65 °C for 8 h if the spool was exposed to >50% RH for >72 h |
| Removal | Let the plate cool before removing parts; remove the brim and deburr |

- With 5 wall loops and 5 top/bottom shells at 0.2 mm, the 2.0 mm walls are fully solid and the solid base zone prints its 1 mm bottom and top shells around 40% infill; the infill setting only matters for that base band and any other region thicker than the shells.
- Do not substitute PLA, ABS, PETG-CF, or PETG HF for production bodies. PETG Basic is the standard because its impact and layer strengths are known and validated by the coupon.
- Do not use variable layer height or adaptive layers on the lip or lid interface.
- The 4 × 4 body is 167.5 mm wide and leaves about 44 mm per side on the X2D bed, so the 5 mm brim fits comfortably.
- Expect roughly 200–500 g and 8–20 hours per body depending on height; the generator reports exact estimates.

## 6. Tolerances and fit classes

| Feature | Fit | Value |
| --- | --- | --- |
| Gridfinity base and lip | Sliding | 0.25 mm per side (standard) |
| Lid skirt | Friction | 0.30 mm per side |
| Lid friction ribs (optional) | Interference | 0.15 mm |
| Bottom edge chamfer | Elephant foot relief | 0.6 mm |
| Printed feature tolerance | — | ±0.2 mm |

- PETG shrinkage of 0.2–0.4% is absorbed by the standard clearances. **Do not scale the model.**
- If the coupon base is tight, increase only the base clearance, up to 0.30 mm per side. Do not change the lip.
- If the lid is loose, add ribs. Do not change the lip.
- Elephant-foot compensation of 0.15–0.20 mm in the slicer is allowed; it is the only slicer compensation that may be tuned.

## 7. Calibration

The coupon is the only iteration. No production box is printed until the coupon passes every acceptance criterion in [`calibration.md`](calibration.md).

The coupon validates: base fit, lip fit, stack fit, lid friction, the 2.0 mm walls, the solid base (filled bottom layers, no bridging), the corner radii, the divider junction, and the floor under load. Print two coupons before the first box. Re-run the coupon after a filament change, a nozzle change, a major slicer upgrade, or any change to the master.

## 8. Sizing rules

1. **Measure the item's maximum bounding box**, including plugs, handles, and coiled cable. For a coil, measure the coil diameter and width, not the cable length.
2. **Add clearance**: 2 mm per side for rigid items, 5 mm per side for soft items and cables, 10 mm where fingers must reach.
3. **Choose the smallest footprint** that fits, from 1 × 1 to 4 × 4. Prefer a footprint that tiles a 4 × 4 plate with the boxes it will share a shelf with.
4. **Choose the height**: default **6U** for general storage; 3U for a single flat layer; 9U for cables; 12U for bulky items. Interior depth must be ≥ item height + 5 mm (rigid) or + 10 mm (soft); leave 2 mm below the lid.
5. **Long items**: length ≤ interior diagonal − 10 mm. The 4 × 4 interior is 163.5 mm, so the diagonal is **231.2 mm**; a 200 mm item fits diagonally.
6. **Stack group**: same footprint, maximum 3 high, 10 kg contents per body.
7. **Division tree**: rectangles only; every leaf at least 12 mm; dividers 2.0 mm; every divider connected to the wall through the tree.

## 9. Furniture compatibility

The system is designed to fit common furniture without knowing the current shelves. The researched reference numbers are in [`docs/research/furniture-dimensions.md`](research/furniture-dimensions.md).

| Furniture | Usable space | What fits |
| --- | --- | --- |
| IKEA Kallax cube | 336 × 336 × 390 mm | Two 2 × 4 bodies (167 mm) or four 2 × 2 bodies; a 4 × 4 body with room to spare |
| IKEA Kallax drawer insert | ~70 mm interior height | A single 6U body (42 mm) easily; 9U (63 mm) with little clearance; not 12U |
| IKEA Billy bookcase | Adjustable shelves, typically ≥200 mm; 30 kg per shelf | A 3-high stack of 6U (126 mm) or 9U (189 mm); 12U prefers 2-high (168 mm) |
| Kitchen / desk drawers | Roughly 70–200 mm | 6U and 9U in most; 12U only in deep drawers |
| Open shelf | ≥200 mm | 3-high 6U or 9U; 2-high 12U |

**Default height: 6U.** It is the most universally compatible single height, and three stacked are 126 mm. Taller boxes are exceptions for cables and bulky items, not the default.

## 10. Naming and records

**System version:** `SYS-<major>.<minor>`. A change to any fixed property bumps the major; a change to the variable properties that does not affect the architecture bumps the minor.

**Box ID:** `<footprint>-<height>-<slug>-<rev>`, for example `4x4-6U-usb-cables-v1`.

**Box folder:**

```
boxes/<box-id>/
  spec.yaml        source of truth for this box, including the division tree
  box.step         archived solid model containing body and lid
  body.stl         print file for the body
  lid.stl          print file for the lid
  notes.md         contents list, print date, filament, measurements
```

**Generated build files** live in `build/<box-id>/` and are never edited by hand; they include the generated Python script, the `.FCStd` review file, and the raw exports. `boxes/INDEX.md` is generated and lists every box with its footprint, height, contents, stack group, and print date.

**Record per box:** print date, filament spool/lot, nozzle, coupon result, actual mass and print time, contents list, stack group.

## 11. Workflow

The step-by-step workflow, the generator contract, and the FreeCAD patterns are in [`workflow.md`](workflow.md) and [`generator.md`](generator.md). The generator is a convenience; the standard must remain usable with FreeCAD alone.

## 12. Confirmed decisions

`SYS-1.0` was frozen on 2026-10-17. All of the following are locked; any change requires a system version bump and an ADR.

One pre-release correction is recorded in [ADR-0013](adr/0013-solid-base.md): the base zone is solid (rows 1 and 5 below), decided before any box was produced and before the coupon passed, so the system version stays SYS-1.0 for that single amendment.

| # | Decision | Value |
| --- | --- | --- |
| 1 | Shell thickness | 2.0 mm walls; solid 4.75 mm base zone with the floor as its top face |
| 2 | Compartment model | Recursive division tree with 1 mm positions |
| 3 | Divider height | Interior depth − 10 mm; 3U uses interior depth − 2 mm |
| 4 | Default height | 6U; 3U/9U/12U as exceptions |
| 5 | Floor | Top of the solid base zone; no added slab, no deflection fallback (ADR-0013) |
| 6 | Corner radii | Outer 3.75, inner 1.75, joints 2.0, lid 1.40 |
| 7 | Generator | Rust CLI, YAML specs, FreeCAD Python master, `freecadcmd` |
| 8 | Labels | None |
| 9 | Maximum footprint | 4 × 4 module |
| 10 | Stack limit | Same footprint only; maximum 3 high; 10 kg per body |

## 13. References

- Gridfinity: <https://gridfinity.xyz/>
- Gridfinity technical drawings: <https://github.com/Stu142/Gridfinity-Documentation>
- FreeCAD Gridfinity Workbench: <https://github.com/Stu142/FreeCAD-Gridfinity-Workbench>
- Bambu Lab X2D specifications: <https://bambulab.com/en/x2d/specs>
- Bambu Lab PETG Basic: <https://uk.store.bambulab.com/products/petg-basic>
- PETG Basic technical data sheet: <https://store.bblcdn.com/s1/default/cb94589bf7994fdcbfa833badefae9cd/Bambu_PETG_Basic_Technical_Data_Sheet.pdf>
- Wall thickness research: [`docs/research/gridfinity-wall-thickness.md`](research/gridfinity-wall-thickness.md)
- Furniture research: [`docs/research/furniture-dimensions.md`](research/furniture-dimensions.md)
