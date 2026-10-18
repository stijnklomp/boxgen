# `boxgen serve` — manual walkthrough

The human checklist for the local web UI (ticket #19), mirroring the
browser-walkthrough ticket (#22). No browser automation exists by decision
(ADR-0002): a person runs this list once per release candidate and records
the result below.

Start the UI with the one-command launcher (ticket #20), or run the binary
directly inside the pinned image:

```bash
just ui                # or scripts/boxgen-ui.sh; mounts $PWD at /app
boxgen serve           # binds 127.0.0.1:8787 and prints the URL
```

Prepare a scratch workspace first if you do not want to touch real boxes:
`cd` into a copy of the repository (or pass `--workdir DIR` to the launcher,
or set `BOXGEN_UI_HOST_WORKDIR` for a direct run, so the shown host paths
point at your mount).

The launcher shares the host network namespace (`--network host`) because
`boxgen serve` binds the container's loopback only; that needs a Linux host.
Docker Desktop runs containers in a VM and does not share the host network, so
run `boxgen serve` directly inside the pinned image there instead.

Record every step: **OK**, **FAIL** (with the exact message), or **N/A**.
Anything unintuitive becomes a follow-up issue — do not fix it in place while
walking.

| # | Step | Expected observation | Result |
| --- | --- | --- | --- |
| 1 | Start the UI and open the printed URL. | The browser shows the box list, `SYS-1.0` in the header, and no console errors. | |
| 2 | Click **New box**, give it an id such as `1x1-6U-walkthrough-v1`. | The editor opens on the calibrated preset: badge `calibrated preset: 4 × 4 · 6U · lid · ribs`, amber `unsaved changes`. | |
| 3 | In **Spec**, set footprint 2 × 4 and height 9U. | The badge turns amber `custom: …` with a one-line diff; the plan redraws; the **Estimate** line under the outer/interior readout updates its mass and print time. | |
| 4 | Click **Reset to preset**. | The badge returns to the calibrated preset and the diff disappears. Reset is visible the whole time. | |
| 5 | Set footprint 2 × 4 · 6U, keep lid + ribs. | The outer/interior readout matches 83.5 × 167.5 mm. | |
| 6 | Click inside the single compartment and press **Split ↔ x**; then click the left compartment and press **Split ↕ y**. | Two dividers appear; the first split's near child kept the original compartment name; the plan stays `valid`. | |
| 6a | Drag from a side of a compartment into it and release. | A divider preview follows the pointer with a live integer-mm label, and releasing adds the division where it was released; the plan stays `valid`. | |
| 6b | Click a side of a compartment without dragging. | A division appears in the middle of that compartment; a side click never selects the compartment. | |
| 7 | Drag the x divider to the left until it stops. | The live label shows integer mm and stops at the 12 mm minimum; it can never cross into the wall. | |
| 8 | Click a divider, type `20` in **Position** and press Enter. | The divider moves to exactly 20 mm; the toolbar shows the legal range. | |
| 9 | Type `2` in **Position** and press Enter. | The value is clamped into the legal range; nothing red appears. | |
| 10 | Drag a divider until a compartment is between 12 and 15 mm. | An amber warning names the small compartment; the spec stays valid. | |
| 11 | Select a compartment and **Rename** it; **Merge away** a compartment whose sibling is also named. | Rename shows the new name in the plan; merge asks which of the two names survives and the chosen one is kept. | |
| 12 | Press Ctrl/Cmd+Z and Ctrl/Cmd+Shift+Z several times. | Undo and redo step through the whole history (at least 100 steps) and every step re-plans through the server. | |
| 13 | Open the **Fit assistant**, enter a 200 × 120 × 60 mm item, and click a suggested combo. | Viable footprint × height chips appear; one click applies the combo and records the contents; `contents.*` warnings appear live if the item no longer fits. | |
| 14 | Set **Contents** height above the interior depth and watch the warnings. | An amber `contents.max_height_mm` warning appears while the spec stays valid. | |
| 15 | Click **Save**. | The badge clears; the toast shows `boxes/<id>/spec.yaml`. | |
| 16 | Refresh the browser and reopen the box from the list. | The saved spec loads; no draft is restored. | |
| 17 | Edit something (do not save) and refresh the browser, then reopen the box. | The draft is restored with the `restored draft` and `unsaved changes` badges; **Discard draft** reloads the file from disk. | |
| 18 | Click **Save as**, accept the suggested new id. | A second box folder is written and the editor switches to it. | |
| 19 | Click **Build** and watch the panel. | The stage advances (`preparing` → `building` → `verifying`) with elapsed seconds, then the summary shows dimensions, mass, print time, floor fillet, and the artifact paths. | |
| 20 | Click **Preview in 3D**. | The viewer shows the real meshes; **Body** and **Lid** toggles work; drag orbits and the wheel zooms; **Back to editing** and **Export** both work. | |
| 21 | In the editor, change the height, then press the browser Back button to return to the preview URL. | The amber stale banner appears with **Rebuild preview**; the editor also shows the `3D preview stale` badge. | |
| 22 | Click **Export** from the viewer (or the editor). | `exports/<base>/` holds `<base>-body.stl`, `<base>-lid.stl` (lid specs), `<base>.step`, `<base>.FCStd`; every path is shown in full and **copy** copies it. | |
| 23 | Open the exported STL in Bambu Studio and the STEP in FreeCAD. | The STL imports and slices with the standard recipe; the STEP opens with `Body` and `Lid` objects. | |
| 24 | Click **Publish**, then look at `boxes/<id>/`. | `box.step`, `body.stl`, `lid.stl` (lid specs), and `notes.md` are copied into `boxes/<id>/`; `spec.yaml` is unchanged; an existing `notes.md` is never overwritten. A box opened from a folder with a legacy name is refused here with a clear message (open still works; **Save as** creates the id-named folder). | |
| 25 | Click **Regenerate index** and open `boxes/INDEX.md`. | The row for the box appears with footprint, height, contents, stack group, and print date; a broken spec in the registry blocks the whole index with a clear error. | |
| 26 | Open the **System standard** view. | Every fixed value is shown read-only and matches the calibration: 2.0 walls, 4.75 base, 3.75/1.75 radii, 2.0 dividers with 12/15 mm leaf limits, 15/28/49/70 mm tops, lid 2.0/2.0/0.30, ribs 0.15, PETG Basic, coupon `coupon-1x1-6U-v1` dated 2026-10-18. | |
| 27 | Check the **Print recipe (fixed)** panel in the editor. | The static reminder matches `docs/spec/docs/standard.md` §5 (0.20 mm layers, 5 wall loops, 5 shells, 40% gyroid, no supports, 5 mm brim, textured PEI, the PETG temperature profile, and the drying rule). | |
| 28 | Put a broken spec (for example a split at `at_mm: 8`) into `boxes/broken-v1/spec.yaml` and refresh the list. | The row is flagged red with its first error; **Open** shows the errors (and the raw text for a spec that does not parse) with **Start from the calibrated preset**. | |
| 29 | Trigger a failed build and click **Build**. For the Docker launcher, put a failing script in the workdir (print `FreeCAD 1.0.0` for `--version`, then exit non-zero) and start with `BOXGEN_FREECADCMD=/app/<script> just ui`; the launcher forwards the variable, and `/app` is the mounted workdir. | The panel shows the exit-contract message plus the `freecadcmd` diagnostic tail; nothing is published. | |
| 30 | Run `just check-docker` on the final commit. | The gate is green; the golden scripts and the CLI output are unchanged. | |

## Result

- Build/binary or image: …
- Date: …
- `just check-docker` on the commit under test: …
- Steps with FAIL or N/A: …
- Follow-up issues filed: …

The walkthrough does not close ticket #22; it feeds it.
