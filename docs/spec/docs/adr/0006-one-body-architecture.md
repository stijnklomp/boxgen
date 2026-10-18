# One body architecture; only the division tree varies

All bodies share one fixed outer architecture: Gridfinity base, a solid base zone with the floor as its top face (ADR-0013), 2.0 mm walls, inset friction lid. Footprint (1 × 1 to 4 × 4) and height (3U–12U) may vary, but the interface dimensions do not. The division tree is the only per-box variable. This is the "one strong design": print the architecture once, then generate layouts.

**Consequences:** any change to the architecture invalidates every printed body and requires a system version bump (ADR-0013 is the single pre-release exception: no body had been printed, so the solid base kept SYS-1.0). The generator enforces the architecture; it does not expose wall thickness, lip dimensions, or lid clearance as free parameters. Only the footprint, height, and division tree vary.
