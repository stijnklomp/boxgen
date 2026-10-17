"""Headless radii and internal-fillet measurements for the SYS-1.0 body.

Run under the pinned launcher:

    freecadcmd cad/tests/radii.py

The script covers the ticket #7 contract:

* ``cad/fillets.py`` applies the standard's reduce-then-fail rule: try the
  2.0 mm radius, accept it only when the result is valid, contains exactly one
  solid, and has positive volume; else try the 1.0 mm fallback with the same
  checks; else raise ``GeometryError``. A sharp internal corner above the base
  is never acceptable.
* ``cad/shell.py::measure_interior_depth_mm`` measures the usable depth from
  the built body (floor top to lip bottom) for every legal height; the report
  carries that measurement, and ``interior_depth_mm`` stays the standard's
  design formula (``U * 7 - 8.75``).
* The built 1x1x6U and 1x1x3U bodies: horizontal slices measure the outer
  3.75 mm and inner 1.75 mm vertical corner arcs and the lip rows
  (``OUTER_RADIUS - offset``), a vertical slice measures the 2.0 mm floor
  cove, and every circular edge above the base is at least 1.0 mm. The hidden
  base cavity keeps the pinned 0.4 mm minimum.

Why synthetic shapes for the reduce and failure paths: the real 1x1 3U
interior cutter -- the smallest legal interior -- still takes the 2.0 mm
fillet (measured below), so no legal spec can force the fallback. A
10x10x1.5 mm prism forces reduce (a 2.0 mm fillet cannot attach to a 1.5 mm
tall bottom edge, a 1.0 mm fillet can) and a 10x10x0.5 mm prism forces the
failure (both radii exceed the edge height).

The script prints one marker line:

    BOXGEN_JSON:{...}

``tests/body_radii.rs`` parses that line and asserts the SYS-1.0 numbers.
``freecadcmd`` prints its banner to stdout, discards unflushed buffers when
the script returns, and exits 0 on an unhandled exception, so the marker
print flushes and the script exits non-zero on any failure.
"""

from __future__ import annotations

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(os.path.dirname(HERE))
if REPO_ROOT not in sys.path:
    sys.path.insert(0, REPO_ROOT)

from cad.tests import _harness as harness


def _attempt(shape, radius: float, edges) -> dict[str, object]:
    """Record one raw ``makeFillet`` attempt without any acceptance checks."""
    try:
        candidate = shape.makeFillet(radius, edges)
    except Exception as error:  # noqa: BLE001 - Part.OCCError is generic
        return {"radius_mm": radius, "exception": str(error)}
    return {
        "radius_mm": radius,
        "valid": bool(candidate.isValid()),
        "solids": len(candidate.Solids),
        "volume_mm3": harness.round6(candidate.Volume),
    }


def _attempt_failed(attempt: dict[str, object]) -> bool:
    """True when the attempt raised, or returned an unacceptable shape."""
    if "exception" in attempt:
        return True
    return not (
        attempt["valid"] and attempt["solids"] == 1 and attempt["volume_mm3"] > 0.0
    )


def _fillet_case(module, shape, radius: float, edges) -> dict[str, object]:
    """Record both raw attempts and the helper's accepted result."""
    case: dict[str, object] = {
        "source_volume_mm3": harness.round6(shape.Volume),
        "standard_attempt": _attempt(shape, radius, edges),
        "fallback_attempt": _attempt(shape, module.FILLET_FALLBACK, edges),
    }
    try:
        filled, used = module.fillet_with_fallback(
            shape,
            radius,
            edges,
            fallback=module.FILLET_FALLBACK,
            what="radii test",
        )
    except module.GeometryError as error:
        case["result"] = {
            "raised": True,
            "error_type": type(error).__name__,
            "message": str(error),
        }
    else:
        case["result"] = {
            "raised": False,
            "used_mm": harness.round6(used),
            "valid": bool(filled.isValid()),
            "solids": len(filled.Solids),
            "volume_mm3": harness.round6(filled.Volume),
        }
    return case


def _wire_info(wire) -> dict[str, object]:
    """Bounding sizes and circular-edge radii of one section wire."""
    box = wire.BoundBox
    radii = set()
    for edge in wire.Edges:
        try:
            curve = edge.Curve
        except Exception:  # noqa: BLE001 - boolean edges can be degenerate
            continue
        if curve.TypeId == "Part::GeomCircle":
            radii.add(harness.round6(curve.Radius))
    return {
        "width_mm": harness.round6(box.XLength),
        "depth_mm": harness.round6(box.YLength),
        "radii": sorted(radii),
    }


def _horizontal_section(shape, z: float) -> list[dict[str, object]]:
    """Section wires at height ``z``, smallest width first."""
    import FreeCAD as App

    return sorted(
        (_wire_info(wire) for wire in shape.slice(App.Vector(0.0, 0.0, 1.0), z)),
        key=lambda info: info["width_mm"],
    )


def _vertical_section_radii(shape) -> list[float]:
    """Circular-edge radii in the vertical centre section (the floor cove)."""
    import FreeCAD as App

    radii = set()
    for wire in shape.slice(App.Vector(0.0, 1.0, 0.0), 0.0):
        for edge in wire.Edges:
            try:
                curve = edge.Curve
            except Exception:  # noqa: BLE001
                continue
            if curve.TypeId == "Part::GeomCircle":
                radii.add(harness.round6(curve.Radius))
    return sorted(radii)


def _circle_radii(shape) -> list[dict[str, object]]:
    """Every circular edge with the z range it spans."""
    circles = []
    for edge in shape.Edges:
        try:
            curve = edge.Curve
        except Exception:  # noqa: BLE001
            continue
        if curve.TypeId == "Part::GeomCircle":
            circles.append(
                {
                    "radius_mm": harness.round6(curve.Radius),
                    "z_min_mm": harness.round6(edge.BoundBox.ZMin),
                    "z_max_mm": harness.round6(edge.BoundBox.ZMax),
                },
            )
    return circles


def _measure_body(module, gf, units_x: int, units_y: int, height_u: int) -> dict[str, object]:
    """Build one body and measure its radii and junctions."""
    result = module.build_body(module.BodySpec(units_x, units_y, height_u))
    body = result.shape
    total_h = height_u * module.HEIGHT_UNIT
    z_lip = total_h - gf.LIP_HEIGHT
    z_floor = gf.BASE_HEIGHT

    mid = _horizontal_section(body, z_lip - 3.0)
    just_above_floor = _horizontal_section(body, z_floor + 0.25)
    lip_rows = []
    for z_relative in (-2.4, -1.8, 0.0, 0.7, 2.5):
        offset = gf.lip_inner_offset(z_relative)
        lip_rows.append(
            {
                "z_relative_mm": z_relative,
                "expected_offset_mm": harness.round6(offset),
                "expected_width_mm": harness.round6(gf.outer_width(units_x) - 2.0 * offset),
                "expected_radius_mm": harness.round6(gf.OUTER_RADIUS - offset),
                "measured": _horizontal_section(body, z_lip + z_relative)[0],
            },
        )

    circles = _circle_radii(body)
    above_base = [
        circle for circle in circles if circle["z_min_mm"] >= z_floor - 1e-6
    ]
    base_zone = [
        circle for circle in circles if circle["z_max_mm"] <= z_floor + 1e-6
    ]

    box = body.BoundBox
    return {
        "volume_mm3": harness.round6(body.Volume),
        "outer_x_mm": harness.round6(box.XLength),
        "outer_y_mm": harness.round6(box.YLength),
        "height_mm": harness.round6(box.ZLength),
        "solids": len(body.Solids),
        "is_valid": bool(body.isValid()),
        "floor_fillet_used_mm": harness.round6(result.floor_fillet_mm),
        "sections": {"mid_body": mid, "just_above_floor": just_above_floor},
        "cove_vertical_radii": _vertical_section_radii(body),
        "lip_rows": lip_rows,
        "min_circle_radius_above_base_mm": min(
            circle["radius_mm"] for circle in above_base
        ),
        "min_circle_radius_base_zone_mm": min(
            circle["radius_mm"] for circle in base_zone
        ),
    }


def _measure_interior_depth(module) -> dict[str, object]:
    """The measured usable depth for every legal height (standard section 4.2).

    ``design_mm`` is the standard's formula (``U * 7 - 8.75``); ``measured_mm``
    is ``cad/shell.py::measure_interior_depth_mm`` on the built 1 x 1 body.
    """
    out: dict[str, object] = {}
    for height_u in (3, 6, 9, 12):
        body = module.build_body(module.BodySpec(1, 1, height_u)).shape
        out[str(height_u)] = {
            "design_mm": harness.round6(module.interior_depth_mm(height_u)),
            "measured_mm": harness.round6(module.measure_interior_depth_mm(body, height_u)),
        }
    return out


def main() -> int:
    import cad.fillets as fillets
    import cad.gridfinity as gf
    import cad.shell as shell

    report: dict[str, object] = {
        "module_imported": True,
        "constants": {
            "FLOOR_FILLET": harness.round6(shell.FLOOR_FILLET),
            "FILLET_FALLBACK": harness.round6(fillets.FILLET_FALLBACK),
            "WALL": harness.round6(shell.WALL),
            "INNER_RADIUS": harness.round6(shell.INNER_RADIUS),
            "OUTER_RADIUS": harness.round6(gf.OUTER_RADIUS),
        },
        "interior_depth": _measure_interior_depth(shell),
    }

    # The smallest legal interior (1x1 3U) takes the standard 2.0 mm radius;
    # this is why the reduce path needs the synthetic prisms below.
    inner_width = gf.outer_width(1) - 2.0 * shell.WALL
    inner_3u = gf.rounded_rect_prism(
        inner_width,
        inner_width,
        shell.INNER_RADIUS,
        gf.BASE_HEIGHT,
        3 * shell.HEIGHT_UNIT - gf.LIP_HEIGHT - shell.LIP_CUTTER_GAP,
    )
    report["helper_standard"] = _fillet_case(
        fillets,
        inner_3u,
        shell.FLOOR_FILLET,
        shell.edges_at_z(inner_3u, gf.BASE_HEIGHT),
    )

    # 10 x 10 x 1.5 mm: 2.0 mm cannot attach to the 1.5 mm bottom edge, 1.0
    # mm can, so the helper must return 1.0.
    thin_reduce = gf.rounded_rect_prism(10.0, 10.0, 1.0, 0.0, 1.5)
    report["helper_reduce"] = _fillet_case(
        fillets,
        thin_reduce,
        2.0,
        shell.edges_at_z(thin_reduce, 0.0),
    )

    # 10 x 10 x 0.5 mm: both radii exceed the edge height, so the helper must
    # raise instead of returning a sharp-cornered shape.
    thin_failure = gf.rounded_rect_prism(10.0, 10.0, 1.0, 0.0, 0.5)
    report["helper_failure"] = _fillet_case(
        fillets,
        thin_failure,
        2.0,
        shell.edges_at_z(thin_failure, 0.0),
    )

    report["body_6u"] = _measure_body(shell, gf, 1, 1, 6)
    report["body_3u"] = _measure_body(shell, gf, 1, 1, 3)

    harness.emit(report)
    return 0


harness.run(main)
