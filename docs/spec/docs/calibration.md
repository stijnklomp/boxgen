# Calibration Coupon

**Status:** `SYS-1.0`
**Purpose:** validate the fixed architecture once, before any production box is printed. The coupon is the only iteration.

## 1. What the coupon proves

- The Gridfinity base seats on a baseplate and in another body's lip.
- The 2.0 mm lip accepts a mating base with the standard clearance.
- The 2.0 mm walls print at the specified thickness, and the base zone prints solid: filled bottom layers with no cavity, no bridging, and no stringing underneath.
- The corner radii print cleanly and remove the sharp internal corners.
- The lid friction fit holds during handling without a latch.
- A 2.0 mm divider fuses to the floor and wall without gaps or cracks.
- The solid base does not deflect under the design load.

## 2. Coupon geometry

One coupon is a 1 × 1 body section with:

| Feature | Value |
| --- | --- |
| Footprint | 1 × 1 Gridfinity unit (41.5 mm outer) |
| Height | 6U (42 mm total) |
| Base zone | Standard Gridfinity base profile, solid (ADR-0013) |
| Body zone | 2.0 mm wall, 2.0 mm lip |
| Floor | Top face of the solid base zone at 4.75 mm |
| Divider | One 2.0 mm T-divider fused to the floor and one wall, 10 mm below the lip |
| Outer corner radius | 3.75 mm |
| Inner corner radius | 1.75 mm |
| Joint fillets | 2.0 mm floor/wall and divider junctions |
| Lid | 2.0 mm plate, 2.0 mm skirt × 2.0 mm, 0.30 mm clearance, two 0.15 mm ribs, one 20 mm finger hole |

Print **two coupons and one lid** in the production recipe. The second coupon is for the stack and load tests.

## 3. Test procedure

1. **Base fit.** Place coupon A on a Gridfinity baseplate (if available) or on coupon B's lip. It should seat with slight play, with no rocking and no force required.
2. **Stack fit.** Stack A on B, then lift A off. It should self-centre, have no wobble, and separate with a deliberate pull.
3. **Lid fit.** Insert and remove the lid 20 times, lifting it by the finger hole. Invert the coupon and shake gently; the lid should stay in place with the ribs. If it falls out, add a second pair of ribs or increase the skirt depth; do not add a latch.
4. **Shell thickness.** Measure with calipers: body wall 2.0 ± 0.1 mm, lip wall 2.0 ± 0.1 mm, lid plate 2.0 ± 0.1 mm. Then look at the base's underside: it must be solid — the bottom layers are filled pads with the standard 0.5 mm cell grooves, no cavity opening, no bridging scars, and no strings inside.
5. **Corner radii.** Inspect the outer corners (3.75 mm), body inner corners (1.75 mm), and joint fillets (2.0 mm). There must be no sharp internal corner above the base and no visible under-extrusion at the fillets.
6. **Divider joint.** Press a 5 kg weight with a 20 mm diameter foot centred on the divider top for 1 hour. Inspect for cracks, whitening, or gaps at the floor and wall joints.
7. **Stack load.** Stack two coupons; place 10 kg on a flat plate that spans the top coupon so the load goes through the lip, not through the lid. Leave it for 24 hours at room temperature. Measure the stack height before and after; the change must be less than 0.5 mm. Inspect the bottom coupon's lip and floor for cracks or deflection.
8. **Floor deflection.** With the 10 kg load in place, measure the floor at the centre of the bottom coupon through the open top, if accessible. Deflection must be less than 1.0 mm. The solid base has no cavity to flex, so a reading above 1.0 mm is not a permitted parameter change: record it and stop for a system decision.
9. **Surface.** Check the lip and lid interface for stringing, blobs, or zits that would affect the fit. If present, dry the filament and reprint before continuing.

## 4. Acceptance criteria

| Test | Pass condition |
| --- | --- |
| Base fit | Seats with slight play; no force, no rocking |
| Stack fit | Self-centres; no wobble; separates by hand |
| Lid fit | Stays in place when inverted and shaken; removable by hand |
| Shell thickness | 2.0 ± 0.1 mm wall, lip, and lid plate; the base underside is solid and filled (no cavity, no bridging) |
| Corner radii | 3.75 mm outer, 1.75 mm body inner, 2.0 mm joints; no sharp internal corners above the base |
| Divider joint | No cracks, whitening, or gaps after 5 kg for 1 h |
| Stack load | Height change < 0.5 mm after 10 kg for 24 h; no cracks |
| Floor deflection | < 1.0 mm under 10 kg (the solid base cannot flex; a failure is a system decision) |
| Surface | No stringing or blobs in the fit surfaces |

If any criterion fails, adjust only the relevant parameter — lid clearance, friction ribs, elephant-foot compensation, drying, or flow — and reprint the coupon. Do not change the architecture without an ADR.

## 5. Record

| Date | Filament / spool | Nozzle | Coupon result | Notes |
| --- | --- | --- | --- | --- |
| | | | | |

## 6. When to re-run

- New filament brand or type.
- New nozzle (including 0.6 mm, if ever used).
- Major Bambu Studio version change.
- Any change to the master geometry.
- A new spool if the first box of the spool shows a fit problem.
