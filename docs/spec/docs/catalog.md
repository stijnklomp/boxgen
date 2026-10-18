# Box Catalog

**Status:** `SYS-1.0`. The maximum footprint is 4 × 4; any footprint from 1 × 1 to 4 × 4 is supported. The default height is 6U.

## 1. Footprints

Outer dimensions are `units × 42 mm − 0.5 mm` (the standard Gridfinity clearance). Interior dimensions subtract the 2.0 mm wall on each side.

| Footprint | Outer size | Interior size | Typical use |
| --- | --- | --- | --- |
| 1 × 1 | 41.5 × 41.5 mm | 37.5 × 37.5 mm | Bolts, bits, adapters, single small items |
| 1 × 2 | 41.5 × 83.5 mm | 37.5 × 79.5 mm | Short thin items, drill bits, cable ties |
| 1 × 3 | 41.5 × 125.5 mm | 37.5 × 121.5 mm | Pens, pencils, small rulers |
| 1 × 4 | 41.5 × 167.5 mm | 37.5 × 163.5 mm | 150 mm calipers, full-length pens, long thin items |
| 2 × 2 | 83.5 × 83.5 mm | 79.5 × 79.5 mm | Small tools, tape measure, adapters |
| 2 × 3 | 83.5 × 125.5 mm | 79.5 × 121.5 mm | Multimeter, hand tools, small cable coils |
| 2 × 4 | 83.5 × 167.5 mm | 79.5 × 163.5 mm | Cable coils, medium tools |
| 3 × 3 | 125.5 × 125.5 mm | 121.5 × 121.5 mm | General tools, measuring equipment |
| 3 × 4 | 125.5 × 167.5 mm | 121.5 × 163.5 mm | General tools, larger cable coils |
| 4 × 4 | 167.5 × 167.5 mm | 163.5 × 163.5 mm | General default, cables, bulky items |

The 4 × 4 module is the reference: any combination of the footprints above tiles a 4 × 4 baseplate. Examples: two 2 × 4; four 2 × 2; one 2 × 4 plus two 2 × 2; four 1 × 4; one 4 × 4.

Interior diagonal: **231.2 mm** for 4 × 4. An item can lie diagonally if its length is at most the diagonal minus 10 mm; a 200 mm ruler fits diagonally in a 4 × 4 body.

## 2. Heights

| Height | Total height | Interior depth | Typical use |
| --- | --- | --- | --- |
| 3U | 21.0 mm | 11.45 mm | A single layer of flat items: bolts, bits, adapters |
| 6U | 42.0 mm | 32.45 mm | Default: pens, measuring equipment, hand tools |
| 9U | 63.0 mm | 53.45 mm | Cables, medium tools, power supplies |
| 12U | 84.0 mm | 74.45 mm | Bulky cables, large tools; shelf or deep drawer |

Maximum height is 12U. 6U is the most universally compatible height and the default for new boxes. See [`docs/research/furniture-dimensions.md`](research/furniture-dimensions.md).

## 3. Recommended combinations

| Use case | Footprint | Height | Division tree |
| --- | --- | --- | --- |
| Bolts, screws, small adapters | 1 × 1 or 2 × 2 | 3U | Equal split into 2 × 2 or 3 × 3 compartments |
| Pens and pencils | 1 × 3 or 1 × 4 | 6U | One or two long compartments |
| Calipers and long thin tools | 1 × 4 | 6U | One 163.5 mm compartment |
| Screwdrivers and hand tools | 2 × 3 or 3 × 3 | 6U | Equal rows, one tool per row |
| Measuring equipment | 3 × 3 or 4 × 4 | 6U | Custom divisions per instrument |
| USB and power cables | 2 × 4 or 4 × 4 | 9U | Two to four medium compartments |
| Bulky cables and power bricks | 4 × 4 | 12U | One or two large compartments |

## 4. Stack groups

Bodies stack only with the same footprint. Mixed footprints tile a 4 × 4 baseplate side by side.

| Group | Footprint | Maximum stack | Notes |
| --- | --- | --- | --- |
| A | 4 × 4 | 3 high | General default |
| B | 3 × 4 | 3 high | General |
| C | 2 × 4 | 3 high | Cables and long tools |
| D | 2 × 3 | 3 high | Hand tools |
| E | 1 × 4 | 3 high | Thin long tools |
| F | smaller footprints | 3 high | Light contents; keep stacks shallow |

For 12U bodies, prefer 2-high stacks on shelves with limited clearance; 2 × 84 mm = 168 mm.

## 5. Baseplates

A 4 × 4 Gridfinity baseplate (168 mm square outer, standard profile) is the reference plate for side-by-side placement. Generate baseplates with the pinned Gridfinity profile; do not redraw them. Larger layouts use multiple 4 × 4 plates.
