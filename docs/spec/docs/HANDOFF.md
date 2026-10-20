# Handoff — SYS-1.0 build-out (historical) and SYS-2.0 conformance

**System:** SYS-2.0, approved default 2026-10-20. The SYS-1.0 build-out below is historical.

## SYS-2.0 official conformance

SYS-2.0 adopts the official Gridfinity 4.4 mm stacking lip and height convention (ADR-0015): nominal body height is `U × 7` to the lip bottom, overall height is `U × 7 + 4.4`, and the physical stack pitch is `U × 7 − 0.35`. Lids are top-only (ADR-0016); a lid must be removed before another body is put on top. SYS-1.0 bodies remain mechanically compatible but are legacy height classes: their 12U is 84.0 mm, SYS-2.0's is 88.4 mm. Old lids fit SYS-2.0 bodies because the lid shape relative to the lip bottom is unchanged. The SYS-1.0 calibration record is history; the SYS-2.0 nine-test calibration is accepted as complete (owner sign-off 2026-10-20), so production boxes may be printed.
**Spec repo:** this directory, branch `main`.
**Implementation repo:** [stijnklomp/boxgen](https://github.com/stijnklomp/boxgen), integration branch `sys-1.0`.

## State of play

- SYS-2.0 is frozen: dimensions, tolerances, division model, corner radii, print recipe, and generator contract are settled.
- This repository is the spec only: Markdown, one YAML template, an empty box registry. There is no generator code here.
- `boxgen` is a separate Rust CLI derived from [stijnklomp/rust-template](https://github.com/stijnklomp/rust-template). The template's layering, typed config, logging, error contract, and `just` recipes are reused; its Postgres, Valkey, RabbitMQ, and HTTP server are not used in v1.
- The SYS-2.0 calibration is accepted by owner sign-off (2026-10-20); no separate coupon print is required. The coupon procedure remains for re-calibration.
- Implementation is tracked in [boxgen issues](https://github.com/stijnklomp/boxgen/issues), milestone [SYS-1.0 build-out](https://github.com/stijnklomp/boxgen/milestone/1).
- **Implementation status:** tickets #2–#12 are implemented and merged to the local integration branch `sys-1.0` in the boxgen clone; the two-axis code review (repo standards + SYS-2.0 spec) is complete and all findings are fixed. `just check` passes in the pinned FreeCAD 1.0.0 image (228+ tests). The branch is **local only**: push `sys-1.0` and mark draft PR #15 ready for review, then merge to `main` to close #2–#12. CI has not run yet because the branch has not been pushed.
- This spec repo is local-only. No remote is configured; do not push it.

## Where the spec lives

| File | What it defines |
| --- | --- |
| [`README.md`](../README.md) | Entry point, fixed/variable summary, locked decisions. |
| [`docs/standard.md`](standard.md) | Dimensions, tolerances, material, print recipe, sizing rules, records. |
| [`docs/catalog.md`](catalog.md) | Footprints, heights, use cases, stack groups. |
| [`docs/generator.md`](generator.md) | The `boxgen` contract: YAML schema, division tree, CLI, FreeCAD master, tests. |
| [`docs/workflow.md`](workflow.md) | Manual and generated paths from an item to a recorded box. |
| [`docs/calibration.md`](calibration.md) | Coupon geometry, nine tests, acceptance criteria, record table. |
| [`GLOSSARY.md`](../GLOSSARY.md) | Project vocabulary. Use these terms. |
| [`docs/adr/`](adr/) | The fourteen architecture decisions and their reasoning. |
| [`templates/box-spec.yaml`](../templates/box-spec.yaml) | Copy-ready box spec. |
| [`boxes/README.md`](../boxes/README.md) | Box folder layout. |

**Fixed:** Gridfinity base and stacking lip; 2.0 mm walls; solid 4.75 mm base zone with a 0.8 mm floor slab (ADR-0013, ADR-0014); 2.0 mm integral dividers; inset friction lid; PETG Basic print recipe; 10 kg per body, maximum 3 high; same-footprint stacking; no labels.

**Variable:** footprint 1 × 1 to 4 × 4; height 3U, 6U, 9U, or 12U (default 6U); the recursive division tree.

A change to a fixed property is a system version bump plus an ADR; ADR-0013 and ADR-0014 are the pre-release exceptions (the solid base and the floor slab, still SYS-2.0, because the checkpoints had only exercised the 1 × 1 coupon). A new compartment layout is a new spec, not a design change.

## Human checkpoints

Two tickets are physical and must be run by a human. An agent may prepare the build and instructions but must not close #13 or #14.

**What the human needs:**

- Bambu Lab X2D.
- Bambu Lab PETG Basic.
- 0.4 mm hardened steel nozzle.
- Calipers.
- A 5 kg weight with a 20 mm diameter foot, for the divider-joint test.
- A 10 kg weight and a flat plate that spans the top coupon, for the stack-load test.
- A Gridfinity baseplate.
- 24 h for the stack-load test.
- Bambu Studio loaded with the production recipe: 0.20 mm layers, 5 wall loops, 5 top/bottom shells, 40% gyroid, no supports, 5 mm brim, textured PEI.

### #13 — Calibration coupon

The SYS-2.0 calibration is accepted by owner sign-off (2026-10-20); all nine tests are considered complete. The procedure below remains for re-calibration after a filament, nozzle, slicer, or geometry change: generate the coupon with `boxgen coupon` (#10), print two coupons and one lid, run all nine tests in [`docs/calibration.md`](calibration.md), and record the result.

If a criterion fails, change only the permitted parameter and reprint. The 2.4 mm-floor fallback is gone: the base zone is solid (ADR-0013) with the 0.8 mm floor slab on top (ADR-0014), so a floor-deflection reading above 1.0 mm is a system decision, not a reprint parameter.

### #14 — 4 × 4 × 6U test box

Blocked by #11 and #13. Commit a 4 × 4, 6U test-box spec under `boxes/` with a representative division tree, build and publish it through the normal `boxgen` path, then print two bodies and two lids in the production recipe, once. Check the lid fit on both bodies. The two bodies must stack self-centring, with slight play, no wobble, and separate by hand.

**Lid-in-stack finding (resolved by the top-only rule):** the stack measurement still shows that a base seated at the lip contact position (base bottom 41.65 mm for 6U, 0.35 mm below the 42.0 mm lip bottom) puts its solid underside 3.35 mm below the 45.0 mm lid top; a lid left under another body would be hit. SYS-2.0 therefore makes lids top-only (ADR-0016), which keeps the lid out of the stack load path. The test box must check a two-body stack with no lid between the bodies, and the lid only on the top body.

Fill in the box record and regenerate the index.

## Recording results

| What | Where |
| --- | --- |
| Coupon result | Calibration record table in [`docs/calibration.md`](calibration.md): date, filament/spool, nozzle, result, notes. |
| Box record | `boxes/<box-id>/notes.md`: contents, print date, filament spool/lot, nozzle, coupon result, actual mass and print time, stack group. |
| Box index | [`boxes/INDEX.md`](../boxes/INDEX.md), generated by `boxgen index`. Never edit it by hand. |
| Build artifacts | `build/<box-id>/`, generated and git-ignored. Only published files are copied into `boxes/<box-id>/`. |

## Definition of done for SYS-1.0 build-out

- [x] Tickets #1–#12 implemented.
- [x] Code tickets #2–#12 merged to the boxgen integration branch `sys-1.0`.
- [x] `just check` passes in boxgen (pinned FreeCAD 1.0.0 image, 228+ tests); CI is green once `sys-1.0` is pushed.
- [x] Code review is clean (both axes run; all findings fixed in `f226445`).
- [x] #13 SYS-2.0 calibration accepted by owner sign-off (2026-10-20).
- [ ] #14 4 × 4 × 6U test box passed and recorded by the human (watch the lid-in-stack finding above).

#13 and #14 stay open until a human runs them. Agent work is complete when the code is merged and the physical checks are ready to run, not when those tickets are marked done.

## How to resume

1. Read [`README.md`](../README.md), [`docs/standard.md`](standard.md), and [`docs/generator.md`](generator.md), in that order. Use [`GLOSSARY.md`](../GLOSSARY.md) for vocabulary.
2. Open the [milestone](https://github.com/stijnklomp/boxgen/milestone/1) and pick an unblocked ticket; each issue lists its blockers. The critical path is #2, then #3 and #4 in parallel, then #5, then #6; #7–#11 build on #6 and #12 on #4.
3. Work in boxgen on `sys-1.0`. Run `just check` before review. Never expose wall thickness, lip dimensions, corner radii, or print settings as user parameters.
4. SYS-2.0 is approved; re-run the coupon after a filament change, a nozzle change, a major slicer upgrade, or any change to the master geometry.
5. Change frozen spec text only with a system version bump and a new ADR under [`docs/adr/`](adr/). Update this handoff freely as state changes.
