"""Headless slicing of an exported box.step (ticket #8 slice evidence).

Run under the pinned launcher with the STEP path and heights in the
environment:

    BOXGEN_STEP=<path/box.step> BOXGEN_SLICES=20,33 freecadcmd cad/tests/step_slices.py

The script loads the exported STEP, finds its ``Body`` object, and reports
the bounding box of every section wire at each height, plus the area each
wire encloses:

    BOXGEN_JSON:{"objects": [...], "section_z20": [[x0, y0, x1, y1], ...],
                 "section_z20_areas": [inner, outer], ...}

``tests/build.rs`` builds the golden tree fixture and asserts the wire boxes
of the exported model (the divider count and positions in the artifact, not
just in the report); ``tests/coupon.rs`` uses the areas to prove the coupon's
T-divider material is in the exported model at z = 20 and gone at z = 33. The
script prints one marker line and exits non-zero on any failure;
``freecadcmd`` discards unflushed stdout and exits 0 on an unhandled
exception, so the marker is flushed.
"""

from __future__ import annotations

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(os.path.dirname(HERE))
if REPO_ROOT not in sys.path:
    sys.path.insert(0, REPO_ROOT)

from cad.tests import _harness as harness


def main() -> int:
    import FreeCAD as App
    import Import
    import Part

    step_path = os.environ["BOXGEN_STEP"]
    heights = [
        float(value)
        for value in os.environ.get("BOXGEN_SLICES", "20,33").split(",")
        if value.strip()
    ]

    report: dict[str, object] = {"step": os.path.basename(step_path)}
    document = App.newDocument("slices")
    try:
        Import.insert(step_path, document.Name)
        document.recompute()
        candidates = [
            obj
            for obj in document.Objects
            if obj.TypeId == "Part::Feature"
            and hasattr(obj, "Shape")
            and obj.Shape.Volume > 0.0
        ]
        # STEP products import under sanitized object names ("Part__Feature");
        # the product name survives as the object's label.
        report["objects"] = sorted(obj.Label for obj in candidates)
        body = next((obj.Shape for obj in candidates if obj.Label == "Body"), None)
        if body is None and candidates:
            body = max(
                (obj.Shape for obj in candidates), key=lambda shape: shape.Volume
            )
        if body is None:
            raise RuntimeError(
                f"no Body object in {step_path}; found {report['objects']}"
            )
        for z in heights:
            section = body.slice(App.Vector(0.0, 0.0, 1.0), z)
            report[f"section_z{z:g}"] = sorted(
                (harness.wire_box(wire) for wire in section), key=lambda box: (box[0], box[1])
            )
            report[f"section_z{z:g}_areas"] = sorted(
                harness.round6(Part.Face(wire).Area) for wire in section
            )
        report["solids"] = len(body.Solids)
        report["is_closed"] = bool(
            len(body.Shells) == 1 and body.Shells[0].isClosed()
        )
    finally:
        App.closeDocument(document.Name)

    harness.emit(report)
    return 0


harness.run(main)
