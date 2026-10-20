# Lids are top-only; the stack pitch is the measured lip seat

SYS-2.0 lids are **top-only**. A body may carry its lid only when no other body is above it. The upper body's base rests on the lower body's lip, not on the lid. Remove the lid before stacking another body on the box; the upper body's solid base covers the lower box.

This resolves the SYS-1.0 lid-in-stack finding. The measured contact seat puts the next body's base bottom 0.35 mm below the lip bottom. For a 6U body that is 41.65 mm from the body bottom; the lid top is 45.0 mm, so the seated base's solid underside is 3.35 mm below the lid top. A lid left under a body would therefore be hit and would carry the body above, contradicting standard §4.5 and ADR-0005. The top-only rule keeps the lid out of the load path and makes the coupon stack-load test representative: it stacks two coupons with no lid between them.

The physical stack pitch is `U × 7 − 0.35 mm`; the stack height of n bodies is `Σ(U × 7) + 4.4 − (n−1) × 0.35 mm`. The lid adds no outer height. The standard's old claim that the pitch equals the body height is removed.

SYS-1.0's calibration record and its 4.0 mm lip bodies are legacy. The SYS-2.0 nine-test calibration is accepted as complete (owner sign-off 2026-10-20), and SYS-2.0 is the approved default.

**Status:** accepted and approved as the SYS-2.0 default, 2026-10-20. Resolves the human checkpoint #14 finding without accepting lid-carried stacks.
