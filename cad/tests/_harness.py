"""Shared scaffolding for the headless FreeCAD measurement scripts.

Every script in this directory is run under the pinned launcher
(``freecadcmd cad/tests/<script>.py``) and has the same constraints:
``freecadcmd`` prints its banner and progress noise to stdout, discards
unflushed buffers when the script returns, and exits 0 on an unhandled
exception. Each script therefore measures the built geometry, prints exactly
one marker line that the Rust integration tests parse:

    BOXGEN_JSON:{...}

and exits non-zero on any failure. This module keeps that scaffolding, the
JSON rounding, the thin-slab section measurement, and the material probes in
one place. Import it as a package module after putting the repository root on
``sys.path``::

    from cad.tests import _harness as harness

The helpers import ``FreeCAD``/``Part`` lazily inside the functions that need
them, so importing this module is safe from any measurement script.
"""

from __future__ import annotations

import json
import sys
import traceback

#: Marker the Rust integration tests search for in the launcher's stdout.
JSON_PREFIX = "BOXGEN_JSON:"


def round6(value: float) -> float:
    """Round a measured value to the report's six-decimal precision."""
    return round(float(value), 6)


def wire_box(wire) -> list[float]:
    """``[x_min, y_min, x_max, y_max]`` of a wire's bounding box."""
    box = wire.BoundBox
    return [round6(box.XMin), round6(box.YMin), round6(box.XMax), round6(box.YMax)]


def slab_section(shape, z: float, thickness: float = 0.02) -> dict[str, object] | None:
    """Measure a thin horizontal slab of ``shape`` at ``z``.

    Returns ``{"z", "width_mm", "depth_mm", "area_mm2", "wires"}`` or ``None``
    when the slab is empty. ``area_mm2`` is the slab volume over its
    thickness; ``wires`` are the section-contour wires of the largest planar
    face, smallest first, so a ring (body wall, lid skirt) yields its inner
    opening and outer face.
    """
    import FreeCAD as App
    import Part

    size = 500.0
    slab = Part.makeBox(
        size,
        size,
        thickness,
        App.Vector(-size / 2.0, -size / 2.0, z - thickness / 2.0),
    )
    common = shape.common(slab)
    if common.isNull() or not common.Solids:
        return None

    wires: list[object] = []
    horizontal = [
        face for face in common.Faces if abs(face.BoundBox.ZLength) < 1e-6
    ]
    if horizontal:
        # The largest planar face of the slice carries the section contour.
        section = max(horizontal, key=lambda face: face.Area)
        wires = sorted(
            section.Wires,
            key=lambda wire: (wire.BoundBox.XLength, wire.BoundBox.YLength),
        )

    box = common.BoundBox
    return {
        "z": round6(z),
        "width_mm": round6(box.XLength),
        "depth_mm": round6(box.YLength),
        "area_mm2": round6(common.Volume / thickness),
        "wires": wires,
    }


def point_is_material(
    shape, x: float, y: float, z: float, size: float = 0.05, threshold: float = 0.9
) -> bool:
    """True when ``shape`` is solid around ``(x, y, z)``."""
    import FreeCAD as App
    import Part

    probe = Part.makeBox(
        size,
        size,
        size,
        App.Vector(x - size / 2.0, y - size / 2.0, z - size / 2.0),
    )
    return bool(shape.common(probe).Volume > probe.Volume * threshold)


def bisect_x(shape, y: float, z: float, material_x: float, void_x: float) -> float:
    """Bisect the material boundary along x between a solid and a void point.

    A 0.01 mm probe reaches half material when its centre is on the surface,
    so the 0.5 threshold finds the true face to within the probe size.
    """
    for _ in range(24):
        middle = (material_x + void_x) / 2.0
        if point_is_material(shape, middle, y, z, size=0.01, threshold=0.5):
            material_x = middle
        else:
            void_x = middle
    return (material_x + void_x) / 2.0


def bisect_y(shape, x: float, z: float, material_y: float, void_y: float) -> float:
    """Bisect the material boundary along y between a solid and a void point."""
    for _ in range(24):
        middle = (material_y + void_y) / 2.0
        if point_is_material(shape, x, middle, z, size=0.01, threshold=0.5):
            material_y = middle
        else:
            void_y = middle
    return (material_y + void_y) / 2.0


def measure_top(shape, x: float, y: float, low: float, high: float) -> float:
    """Bisect the material/air boundary at ``(x, y)`` between two heights.

    ``low`` is inside material and ``high`` inside void; the result is the
    highest z of the material (the top of a solid feature).
    """
    for _ in range(24):
        middle = (low + high) / 2.0
        if point_is_material(shape, x, y, middle, size=0.01, threshold=0.5):
            low = middle
        else:
            high = middle
    return (low + high) / 2.0


def bisect_z(shape, x: float, y: float, low: float, high: float) -> float:
    """Bisect the material/air boundary at ``(x, y)`` between two heights.

    ``low`` is inside void and ``high`` inside material; the result is the
    lowest z of the material (the ceiling of a void, like a base's underside
    cavity).
    """
    for _ in range(24):
        middle = (low + high) / 2.0
        if point_is_material(shape, x, y, middle, size=0.01, threshold=0.5):
            high = middle
        else:
            low = middle
    return (low + high) / 2.0


def emit(report: dict[str, object]) -> None:
    """Print the one marker line the Rust tests parse (flushed)."""
    print(JSON_PREFIX + json.dumps(report, sort_keys=True), flush=True)


def run(main) -> None:
    """Run a measurement ``main`` with the launcher-safe exit contract."""
    try:
        sys.exit(main())
    except SystemExit:
        raise
    except BaseException:  # noqa: BLE001 - freecadcmd hides exceptions otherwise
        traceback.print_exc()
        sys.exit(1)
