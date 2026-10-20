"""Headless measurements for the pinned Gridfinity interface.

Run under the pinned launcher:

    freecadcmd cad/tests/interface.py

The script imports ``cad/gridfinity.py`` -- which must work with no Gridfinity
workbench installed -- builds the base foot, the lip void, and the lip ring,
measures them, and prints one marker line:

    BOXGEN_JSON:{...}

``tests/gridfinity_interface.rs`` parses that line and asserts the SYS-1.0
numbers. ``freecadcmd`` prints its banner and progress noise to stdout, so
the marker keeps the JSON unambiguous. The script exits non-zero on any
failure, which matters because ``freecadcmd`` swallows unhandled exceptions
and still exits 0.
"""

from __future__ import annotations

import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CAD_DIR = os.path.dirname(HERE)
REPO_ROOT = os.path.dirname(CAD_DIR)
for path in (CAD_DIR, REPO_ROOT):
    if path not in sys.path:
        sys.path.insert(0, path)

from cad.tests import _harness as harness


def _bbox(shape) -> dict[str, float]:
    box = shape.BoundBox
    return {
        "width": harness.round6(box.XLength),
        "depth": harness.round6(box.YLength),
        "height": harness.round6(box.ZLength),
        "zmin": harness.round6(box.ZMin),
        "zmax": harness.round6(box.ZMax),
    }


def _section(shape, z: float) -> dict[str, object]:
    """Measure the cross-section at ``z``.

    Planar edges at ``z`` are the exact profile wires and work on boundary
    faces; when ``z`` falls inside a straight segment there are none, so the
    section is sliced instead. Both paths aggregate the bounding box and the
    circular-edge radii of the section.
    """
    parts: list[object] = [
        edge
        for edge in shape.Edges
        if abs(edge.BoundBox.ZMin - z) < 1e-6 and abs(edge.BoundBox.ZMax - z) < 1e-6
    ]
    if not parts:
        import FreeCAD as App

        parts = list(shape.slice(App.Vector(0.0, 0.0, 1.0), z))
    if not parts:
        raise RuntimeError(f"no cross-section at z={z}")

    boxes = [part.BoundBox for part in parts]
    radii = set()
    for part in parts:
        for edge in part.Edges:
            try:
                curve = edge.Curve
            except Exception:  # noqa: BLE001 - boolean edges can be degenerate
                continue
            if curve.TypeId == "Part::GeomCircle":
                radii.add(harness.round6(curve.Radius))
    return {
        "z": harness.round6(z),
        "widths": [
            harness.round6(max(box.XMax for box in boxes) - min(box.XMin for box in boxes)),
        ],
        "depths": [
            harness.round6(max(box.YMax for box in boxes) - min(box.YMin for box in boxes)),
        ],
        "radii": sorted(radii),
    }


def _loaded_workbench_modules() -> list[str]:
    return sorted(name for name in sys.modules if "gridfinity_workbench" in name)


def _workbench_importable() -> bool:
    for name in ("gridfinity_workbench", "freecad.gridfinity_workbench"):
        try:
            if importlib.util.find_spec(name) is not None:
                return True
        except (ImportError, ModuleNotFoundError, ValueError):
            continue
    return False


def main() -> int:
    import gridfinity as gf

    report: dict[str, object] = {
        "module_imported": True,
        "module_file": os.path.abspath(gf.__file__ or ""),
        "workbench_importable": _workbench_importable(),
        "loaded_workbench_modules": _loaded_workbench_modules(),
        "constants": {
            name: harness.round6(getattr(gf, name))
            for name in (
                "GRID",
                "CLEARANCE",
                "NOMINAL_WIDTH_1U",
                "BASE_HEIGHT",
                "BASE_BOTTOM_CHAMFER",
                "BASE_VERTICAL",
                "BASE_TOP_CHAMFER",
                "BASE_BOTTOM_RADIUS",
                "BASE_VERTICAL_RADIUS",
                "OUTER_RADIUS",
                "LIP_HEIGHT",
                "LIP_BOTTOM_CHAMFER",
                "LIP_VERTICAL",
                "LIP_TOP_CHAMFER",
                "LIP_TOP_LEDGE",
                "LIP_WALL",
                "LIP_OVERLAP",
            )
        },
        "outer_width": {str(units): harness.round6(gf.outer_width(units)) for units in (1, 2, 4)},
    }

    # Base: one cell then a footprint, at the workbench's four profile rows.
    foot = gf.make_base_foot()
    base_sections = []
    for z in (
        0.0,
        gf.BASE_BOTTOM_CHAMFER,
        gf.BASE_BOTTOM_CHAMFER + gf.BASE_VERTICAL,
        gf.BASE_HEIGHT,
    ):
        section = _section(foot, z)
        base_sections.append(
            {
                "z": section["z"],
                "width": section["widths"][-1],
                "depth": section["depths"][-1],
                "radii": section["radii"],
            },
        )
    report["base_foot"] = {"bbox": _bbox(foot), "sections": base_sections}

    feet: dict[str, object] = {}
    for label, footprint in (("2x1", (2, 1)), ("4x4", (4, 4))):
        shape = gf.make_base_feet(footprint)
        bbox = _bbox(shape)
        bbox["solids"] = len(shape.Solids)
        feet[label] = bbox
    report["base_feet"] = feet

    # Lip: the profile, its offset function, the void, and the ring.
    one_unit = gf.rounded_rect_wire(gf.outer_width(1), gf.outer_width(1), gf.OUTER_RADIUS)
    cut = gf.make_lip_cut(one_unit)
    cut_sections = []
    for z in (
        -2.4,
        -1.8,
        0.0,
        gf.LIP_BOTTOM_CHAMFER,
        gf.LIP_BOTTOM_CHAMFER + gf.LIP_VERTICAL,
        gf.LIP_HEIGHT,
    ):
        section = _section(cut, z)
        cut_sections.append(
            {
                "z": section["z"],
                "width": section["widths"][0],
                "depth": section["depths"][0],
                "radii": section["radii"],
            },
        )

    ring = gf.make_stacking_lip(one_unit)
    rect_wire = gf.rounded_rect_wire(gf.outer_width(2), gf.outer_width(1), gf.OUTER_RADIUS)
    rect_ring = gf.make_stacking_lip(rect_wire)
    report["lip"] = {
        "profile": [[harness.round6(x), harness.round6(z)] for x, z in gf.lip_profile_points()],
        "offsets": [
            {"z": harness.round6(z), "offset": harness.round6(gf.lip_inner_offset(z))}
            for z in (-2.4, -1.8, 0.0, 0.7, 2.5, 4.4)
        ],
        "cut_sections": cut_sections,
        "ring": {
            "bbox": _bbox(ring),
            "height_above_bottom": harness.round6(ring.BoundBox.ZMax),
            "overlap_below_bottom": harness.round6(-ring.BoundBox.ZMin),
            "volume": harness.round6(ring.Volume),
        },
        "rect_ring": {"bbox": _bbox(rect_ring)},
    }

    harness.emit(report)
    return 0


harness.run(main)
