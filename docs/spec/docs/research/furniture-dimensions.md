# Furniture dimensions — research note

**Question:** what box and stack sizes are most compatible with common shelves and drawers?

**Findings (2026-10-17):**

| Furniture | Usable space | Source type |
| --- | --- | --- |
| IKEA Kallax cube | 336 × 336 × 390 mm | Retailer measurement guides |
| IKEA Kallax drawer insert | ~70 mm interior height | Retailer/secondary guides |
| IKEA Billy bookcase | Adjustable shelves, typically ≥200 mm clearance; 30 kg shelf load | Retailer specs/secondary guides |
| IKEA Alex drawer unit | 45 cm deep unit, 39 cm drawer inside depth, 4 kg max per drawer; shallow drawers | Retailer specs |
| Kitchen drawers | Roughly 80–200 mm internal height | General joinery range |
| Desk drawers | Roughly 70–120 mm internal height | General range |

**Implications for this system:**

- The **4 × 4 module (167.5 mm)** divides a Kallax cube cleanly: two 2 × 4 bodies or four 2 × 2 bodies are 167 mm across, leaving ~169 mm of the 336 mm cube free. A single 4 × 4 body also fits with room.
- **6U (46.4 mm overall)** is the most universally compatible height. Three 6U bodies stacked are 129.7 mm, which fits a Billy shelf with ≥200 mm clearance and a Kallax cube with 336 mm. A single 6U body fits a ~70 mm drawer insert.
- **9U (67.4 mm overall)** fits most drawers and shelves; three stacked are 192.7 mm. Use it for cables.
- **12U (88.4 mm overall)** does not fit shallow drawers; use it for bulky items and prefer 2-high stacks (172.05 mm) on shelves. SYS-1.0 12U prints are 84 mm and are legacy.
- **3U (25.4 mm overall)** is for flat contents only; it is the most space-efficient for a single layer of small parts.
- A 4 × 4 footprint leaves about 44 mm of bed margin per side on the X2D, so brims and first layers are comfortable.

**Decision:** the default height is **6U**, with 3U, 9U, and 12U as options, and the maximum footprint is **4 × 4**. See [ADR-0011](../adr/0011-4x4-module.md).

**Sources:**
- IKEA Kallax interior dimensions: <https://www.manualslib.com/answers/4499912/ikea-kallax-internal-dimensions-and-cube-size-guide.html>
- IKEA Billy buying guide: <https://www.ikea.com/gb/en/files/pdf/e0/30/e03064f8/billy_buying_guide_a4.pdf>
- IKEA Alex drawer unit: <https://www.ikea.com/gb/en/p/alex-drawer-unit-white-60586908/>
- Standard bookshelf clearance: <https://www.sizemarker.com/dimensions/standard-bookshelf-dimensions>
