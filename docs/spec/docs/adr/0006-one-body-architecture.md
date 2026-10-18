# One body architecture; only the division tree varies

All bodies share one fixed outer architecture: Gridfinity base, 2.0 mm walls and floor, 1.2 mm base side shell, inset friction lid. Footprint (1 × 1 to 4 × 4) and height (3U–12U) may vary, but the interface dimensions do not. The division tree is the only per-box variable. This is the "one strong design": print the architecture once, then generate layouts.

**Consequences:** any change to the architecture invalidates every printed body and requires a system version bump. The generator enforces the architecture; it does not expose wall thickness, lip dimensions, or lid clearance as free parameters. Only the footprint, height, and division tree vary.
