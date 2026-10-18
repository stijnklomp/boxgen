## Calibration coupon — iteration 1: GO

- **Date:** 2026-10-18
- **Filament / spool:** PETG Basic 30105
- **Nozzle:** 0.4 mm hardened steel
- **Bambu Studio:** 2_8_2_61
- **Artifacts:** body.stl sha256 6ab95bd86509…, lid.stl sha256 4a8212678901…
- **Record file:** /home/stijn/developer/personal/boxgen/docs/spec/docs/calibration-records/calibration-coupon-2026-10-18-iter1.md

| # | Test | Result | Measurement / note |
| --- | --- | --- | --- |
| 1 | Base fit | PASS | seats with slight play, no rocking |
| 2 | Stack fit | PASS | self-centres, no wobble, separates by hand |
| 3 | Lid fit | PASS | 20 cycles; stays in when inverted and shaken |
| 4 | Shell thickness | PASS | wall 2 mm · lip 2.66 mm · lid plate 4 mm · base underside: All good |
| 5 | Corner radii | PASS | outer 3.75 · inner 1.75 · joints 2.0; no sharp internal corners |
| 6 | Divider joint | PASS | 5 kg for 0 min: no cracks, whitening, or gaps |
| 7 | Stack load | PASS | 10 kg for 0 min; height before/after: N/A → N/A mm |
| 8 | Floor deflection | PASS | N/A mm under 10 kg — N/A |
| 9 | Surface | PASS | no stringing or blobs in the fit surfaces |

### Failures and permitted fixes

_None — all nine criteria held on this print._

**GO** — all nine acceptance criteria pass in iteration 1. The calibration record row (calibration.md §5) is in the record file; paste it into the record table and close #13. #14 (test box) is unblocked.
