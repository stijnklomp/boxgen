> **SYS-1.0 legacy record.** This coupon is 42.0 mm overall with the 4.0 mm workbench lip. SYS-2.0 requires a new coupon (46.4 mm, 4.4 mm lip) and a fresh nine-test run.

# Calibration coupon record — iteration 1

- **Date:** 2026-10-18
- **Filament / spool:** PETG Basic 30105
- **Nozzle:** 0.4 mm hardened steel
- **Bambu Studio:** 2_8_2_61
- **Slicer estimate:** 1h 20m h / 0.74 g
- **Coupon:** coupon-1x1-6U-v1 — 41.5 × 41.5 × 42.0 mm body, ribbed lid, built by `boxgen coupon`
- **Artifacts:** body.stl sha256 6ab95bd86509…, lid.stl sha256 4a8212678901…
- **Procedure:** docs/spec/docs/calibration.md §3–§4 (sha256 4f276f74755f…)
- **Go/no-go:** **GO**

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

## Failures and permitted fixes

_None — all nine criteria held on this print._

## Calibration record row (docs/calibration.md §5)

| 2026-10-18 | PETG Basic 30105 | 0.4 mm hardened steel | PASS | iteration 1; all nine tests pass |

## Notes

This all looks fine. Happy to proceed.
