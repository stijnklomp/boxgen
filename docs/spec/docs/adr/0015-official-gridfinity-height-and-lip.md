# Adopt the official Gridfinity 4.4 mm stacking lip and height convention

SYS-2.0 replaces the SYS-1.0 workbench lip variant with the official Gridfinity stacking lip and height accounting. The nominal body height `U × 7 mm` now runs from the base bottom to the nominal top plane (the lip bottom), **excluding** the lip. The official 4.4 mm lip (`0.7` bottom chamfer + `1.8` vertical + `1.9` top chamfer, horizontal depth `2.6 mm`) sits above that plane, so overall height is `U × 7 + 4.4 mm`. The base profile is unchanged. A base seats 0.35 mm below the nominal top plane, so the physical stack pitch is `U × 7 − 0.35 mm`; a stack of n bodies is `Σ(U × 7) + 4.4 − (n−1) × 0.35 mm`.

This makes `6U + 6U` and a single `12U` differ by 0.35 mm rather than 4.35 mm. The SYS-1.0 lip counted the 4.0 mm lip inside `U × 7`; every lower body's lip was buried by the next base, so each joint lost the full lip height. SYS-1.0 bodies are legacy: they remain mechanically stackable with each other and fit baseplates, but they are not SYS-2.0 height classes. SYS-1.0's 12U is 84.0 mm; SYS-2.0's is 88.4 mm.

The lid shape relative to the lip bottom is unchanged. The lid plate top is `z_lip + SKIRT_INTRUSION + LID_PLATE = U × 7 + 3.0 mm`, which is 1.4 mm below the 4.4 mm lip top. Existing SYS-1.0 lids fit SYS-2.0 bodies because the lip's inner profile below the lid top is unchanged.

The official nominal profile is sharp at the top edge. A print-only tip roundover of at most 0.6 mm is allowed by the official ecosystem, but it must be measured and recorded; it is not a silent change to the nominal height or stack pitch. The first SYS-2.0 build uses the exact sharp profile; the SYS-2.0 calibration was accepted as complete on 2026-10-20.

**Status:** accepted and approved as the SYS-2.0 default, 2026-10-20. Supersedes the lip-profile half of ADR-0010; the base profile remains pinned from the workbench.
