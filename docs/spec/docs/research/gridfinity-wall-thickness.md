# Gridfinity wall thickness — research note

**Question:** what wall thickness do other Gridfinity designers use, and what is the maximum that keeps the standard fit?

**Findings (2026-10-17):**

- The original Gridfinity bins and the FreeCAD Gridfinity Workbench default to **1.0 mm** walls and **1.2 mm** dividers.
- Common community bins use **1.2 mm**, **1.5 mm**, and **2.0 mm** walls. 1.2 mm is the usual "standard" upgrade; 2.0 mm is the usual "heavy duty" choice.
- A widely used Gridfinity designer caps wall thickness at **2.0 mm**; 2.1 mm and above cannot be generated with a standard-compatible lip.
- Some designs use **2.6 mm** walls (for example the KYZ Design "Gridfinity 2.6 mm Wall Divider Bins"), but these deviate from the stock fit or modify the lip; they are not drop-in standard-compatible.
- The constraint is the stacking lip: the standard opening is sized for a thin lip. A wall thickness above about **2.25 mm** makes the opening too small for a mating base to seat.
- The **base zone's side shell** is a second constraint: the standard base profile's chamfers and internal fillets cap the internal side shell at about **1.2 mm** before the fillet collapses. The master applies a 0.4 mm minimum fillet to the hidden cavity. The floor above it can be thicker because it is cut into the top of the base, not offset from the outer profile.
- The base profile and lip are what must stay standard; the wall thickness above the base is the free variable.

**Decision:** use **2.0 mm** for the body, lip, floor, dividers, and lid, and **1.2 mm** for the base zone's side shell (the standard-profile maximum). See [ADR-0007](../adr/0007-maximum-thickness-shell.md).

**Sources:**
- FreeCAD Gridfinity Workbench constants: <https://github.com/Stu142/FreeCAD-Gridfinity-Workbench/blob/master/freecad/gridfinity_workbench/const.py>
- Fusion Gridfinity Generator issue: wall thickness cannot be 2.1 mm or greater: <https://github.com/Le0Michine/FusionGridfinityGenerator/issues/35>
- Printables wall thickness comparison (0.9/1.2/1.5 mm): <https://www.printables.com/model/1305693>
- MakerWorld 2.6 mm wall bins: <https://makerworld.com/en/models/32315>
- Gridfinity Bin Designer wall options (0.4–2.6 mm): <https://gridfinitylayouttool.com/designer>
