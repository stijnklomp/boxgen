# Stackable Storage System

A 3D-printed storage system for household equipment: Gridfinity-compatible bodies with integral compartments, optional friction lids, and a maximum-thickness shell designed for the Bambu Lab X2D in PETG Basic.

**Status:** `SYS-2.0`, approved default 2026-10-20. The architecture, sizing, division model, corner radii, and generator contract are settled; the calibration is accepted as complete. The generator lives in the `boxgen` repository.

## What is fixed, what varies

**Fixed — the architecture:**
- Gridfinity 42 × 42 × 7 mm base and the official 4.4 mm stacking lip; nominal body height excludes the lip.
- 2.0 mm walls; a solid 4.75 mm base zone with a 0.8 mm floor slab on top (ADR-0013, ADR-0014); the base prints filled on the bed.
- Integral dividers connected to the walls; no separate inserts.
- Inset friction lid, 2.0 mm plate and skirt; no latch, magnet, or hinge.
- PETG Basic, 0.4 mm nozzle, 0.2 mm layers, 5 wall loops, 40% gyroid infill.
- Heavy-duty rating: 10 kg contents per body, maximum 3 bodies high.
- Vertical stacking only; nothing protrudes beyond the body footprint.
- No labels, embossed text, or colour coding.

**Variable — the data:**
- Footprint (1 × 1 to 4 × 4 Gridfinity units).
- Height (3U, 6U, 9U, 12U; default 6U).
- Compartment division tree (recursive splits with positions in millimetres).

The fixed part is the "one strong design"; the variable part is generated per box. A change to the fixed part is a system version change and invalidates previously printed bodies.

## Files

| File | Purpose |
| --- | --- |
| [`docs/standard.md`](docs/standard.md) | The main design standard: dimensions, tolerances, material, print recipe, sizing rules. Start here. |
| [`docs/catalog.md`](docs/catalog.md) | Footprints, heights, use-case combinations, stack groups. |
| [`docs/workflow.md`](docs/workflow.md) | How to go from an item to a finished box, manually in FreeCAD or through the generator. |
| [`docs/generator.md`](docs/generator.md) | The `boxgen` contract: YAML schema, division tree, CLI, FreeCAD master, tests. |
| [`docs/calibration.md`](docs/calibration.md) | The calibration coupon and acceptance criteria. SYS-2.0 is accepted; use this for re-calibration after a change. |
| [`templates/box-spec.yaml`](templates/box-spec.yaml) | Copy-ready box spec. |
| [`boxes/README.md`](boxes/README.md) | Box folder layout and generated index. |
| [`docs/research/gridfinity-wall-thickness.md`](docs/research/gridfinity-wall-thickness.md) | Why the walls are 2.0 mm and the base zone is solid. |
| [`docs/research/furniture-dimensions.md`](docs/research/furniture-dimensions.md) | Why 4 × 4 and 6U are the defaults. |
| [`GLOSSARY.md`](GLOSSARY.md) | Project vocabulary: body, lid, lip zone, division tree, stack group, coupon. |
| [`docs/adr/`](docs/adr/) | Architecture decisions and the reasoning behind them. |

## Locked decisions

| Decision | Record |
| --- | --- |
| Gridfinity is the base and stacking interface. | [ADR-0001](docs/adr/0001-gridfinity-interface.md) |
| Bento3D is rejected as the source of truth; it is a UX reference only. | [ADR-0002](docs/adr/0002-bento3d-rejected.md) |
| Compartments are integral to the body, connected to the walls. | [ADR-0003](docs/adr/0003-integral-dividers.md) |
| Compartment maps are recursive division trees. | [ADR-0008](docs/adr/0008-recursive-division-tree.md) |
| Maximum-thickness shell: 2.0 mm walls; solid base zone with the floor as its top face (amends the ADR-0007 base shell). | [ADR-0007](docs/adr/0007-maximum-thickness-shell.md), [ADR-0013](docs/adr/0013-solid-base.md) |
| Inset friction lid; no latch, magnet, or hinge. | [ADR-0005](docs/adr/0005-inset-friction-lid.md) |
| One body architecture; only the division tree varies. | [ADR-0006](docs/adr/0006-one-body-architecture.md) |
| YAML specs plus a FreeCAD Python master; the generator emits FreeCAD Python. | [ADR-0009](docs/adr/0009-yaml-specs-freecad-master.md) |
| Gridfinity profile geometry is pinned in the master. | [ADR-0010](docs/adr/0010-pin-gridfinity-profiles.md) |
| The 4 × 4 module is the maximum footprint. | [ADR-0011](docs/adr/0011-4x4-module.md) |
| No labels or colour coding. | [ADR-0012](docs/adr/0012-no-labels.md) |

## Confirmed decisions

All of the following are locked in `SYS-2.0`:

1. **Shell:** 2.0 mm body, lip, dividers, and lid; a solid 4.75 mm base zone with a 0.8 mm floor slab on top (ADR-0013, ADR-0014); no underside cavity and no floor-thickness fallback.
2. **Division model:** recursive tree with integer-millimetre positions, near-face measured.
3. **Divider height:** interior depth − 10 mm; 3U uses interior depth − 2 mm.
4. **Default height:** 6U; 3U/9U/12U as exceptions; maximum 12U.
5. **Corner radii:** outer 3.75 mm, inner 1.75 mm, joints 2.0 mm, lid 1.40 mm.
6. **Maximum footprint:** 4 × 4 module; any 1 × 1 to 4 × 4 footprint tiles it.
7. **Stack limit:** same footprint only, maximum 3 high, 10 kg per body.
8. **Labels:** none.
9. **Generator:** Rust CLI, YAML specs, FreeCAD Python master, `freecadcmd`, separate repo forked from the Rust template and stripped of unused server infrastructure.

## Next steps

1. Build the `boxgen` repository against the contract in [`docs/generator.md`](docs/generator.md).
2. The SYS-2.0 calibration is accepted; use [`docs/calibration.md`](docs/calibration.md) only for a re-calibration after a filament, nozzle, slicer, or geometry change.
3. Print one 4 × 4 × 6U test box and check the lid and a two-box stack.

## The short version

1. Read [`docs/standard.md`](docs/standard.md).
2. The SYS-2.0 calibration is accepted; re-run the coupon only after a filament, nozzle, slicer, or geometry change.
3. Print one 4 × 4 × 6U test box and check the lid and a two-box stack.
4. Generate production boxes from division-tree specs.
5. Record each print and its contents in the box folder.

## References

- Gridfinity: <https://gridfinity.xyz/>
- FreeCAD Gridfinity Workbench (base profile source): <https://github.com/Stu142/FreeCAD-Gridfinity-Workbench>
- Unofficial Gridfinity specification (lip source): <https://github.com/gridfinity-unofficial/specification>
- Bambu Lab X2D specifications: <https://bambulab.com/en/x2d/specs>
- Bambu Lab PETG Basic: <https://uk.store.bambulab.com/products/petg-basic>
- PETG Basic technical data sheet: <https://store.bblcdn.com/s1/default/cb94589bf7994fdcbfa833badefae9cd/Bambu_PETG_Basic_Technical_Data_Sheet.pdf>
- Bento3D: <https://bento3d.design/> (proprietary; UX reference only)
