"""Headless lid measurements for the SYS-1.0 inset friction lid (ticket #9).

Run under the pinned launcher:

    freecadcmd cad/tests/lid.py

The script builds the standard 2 x 4 x 6U lid (non-square on purpose) and the
matching body, and measures every number the standard fixes:

* ``lid_plain`` / ``lid_ribs``: bbox and z range, plate thickness (slices
  above/below the plate bottom), skirt thickness (slices through the ring:
  outer minus inner = 4.0), plate corner radius 1.40, top roundover 1.0
  (the top planar face is inset by the roundover, the finger hole's top rim
  becomes a 11.0 circle), 0.6 skirt bottom chamfer (slice widths at two
  depths), and the 20 mm finger hole centred 14 mm from the +y edge.
* ``body``: the 2 x 4 x 6U body's lip rows, so the lid clearances can be
  compared against the mating surfaces (0.30 mm per side at the neck and the
  vertical section).
* ``stack``: the next body's base bottom sits 1.0 mm above the lid top at
  the design stack pitch (the lid is not in the load path there); the same
  base is also seated in the lip's measured contact position (base bottom
  0.35 mm below the lip bottom), where its solid underside meets the lid's
  plate -- the documented finding for the human checkpoint (#14): with the
  lid fitted, a stacked body rests on the lid, not on the lip.

The script prints one marker line:

    BOXGEN_JSON:{...}

``tests/lid.rs`` parses that line and asserts the SYS-1.0 numbers.
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

#: Units under measurement: non-square so an x/y mix-up is caught.
UNITS_X = 2
UNITS_Y = 4
HEIGHT_U = 6


def _section(shape, z: float, thickness: float = 0.02) -> dict[str, object]:
    """Slice ``shape`` with a thin slab at ``z`` and measure the section.

    ``wires`` lists the boundary contours of the section, smallest first, so
    a ring (body wall, lid skirt) yields its inner opening and outer face;
    the bbox fields are the whole section.
    """
    slab = harness.slab_section(shape, z, thickness)
    if slab is None:
        return {"z": harness.round6(z), "empty": True}
    return {
        "z": slab["z"],
        "width_mm": slab["width_mm"],
        "depth_mm": slab["depth_mm"],
        "area_mm2": slab["area_mm2"],
        "wires": [
            {
                "width_mm": harness.round6(wire.BoundBox.XLength),
                "depth_mm": harness.round6(wire.BoundBox.YLength),
            }
            for wire in slab["wires"]
        ],
    }


def _circles(shape) -> list[dict[str, object]]:
    """Every circular edge with its radius, centre, and z span."""
    circles: dict[tuple, dict[str, object]] = {}
    for edge in shape.Edges:
        try:
            curve = edge.Curve
        except Exception:  # noqa: BLE001 - boolean edges can be degenerate
            continue
        if curve.TypeId != "Part::GeomCircle":
            continue
        centre = curve.Center
        key = (
            harness.round6(curve.Radius),
            harness.round6(centre.x),
            harness.round6(centre.y),
            harness.round6(edge.BoundBox.ZMin),
            harness.round6(edge.BoundBox.ZMax),
        )
        circles[key] = {
            "radius_mm": key[0],
            "centre_x_mm": key[1],
            "centre_y_mm": key[2],
            "z_min_mm": key[3],
            "z_max_mm": key[4],
        }
    return sorted(
        circles.values(),
        key=lambda item: (
            item["radius_mm"],
            item["z_min_mm"],
            item["centre_x_mm"],
            item["centre_y_mm"],
        ),
    )


def _top_face_width(shape, z_top: float) -> float:
    """Width of the widest planar face at ``z_top`` (the plate top face)."""
    widths = [
        face.BoundBox.XLength
        for face in shape.Faces
        if abs(face.BoundBox.ZMin - z_top) < 1e-6
        and abs(face.BoundBox.ZMax - z_top) < 1e-6
    ]
    return harness.round6(max(widths)) if widths else 0.0


def _measure_lid(lid_module, gf, units_x, units_y, height_u, ribs) -> dict[str, object]:
    """Build one lid and measure the standard's dimensions."""
    shape = lid_module.build_lid(
        lid_module.BodySpec(units_x, units_y, height_u), ribs=ribs
    )
    total_h = height_u * lid_module.HEIGHT_UNIT
    z_lip = total_h - gf.LIP_HEIGHT
    z_top = total_h - 1.0
    z_plate_bottom = z_top - lid_module.LID_PLATE
    z_skirt_bottom = z_lip - lid_module.SKIRT_INTRUSION

    outer_x = gf.outer_width(units_x)
    outer_y = gf.outer_width(units_y)
    plate_x = outer_x - 2.0 * (lid_module.LIP_VERTICAL_OFFSET + lid_module.LID_CLEARANCE)
    plate_y = outer_y - 2.0 * (lid_module.LIP_VERTICAL_OFFSET + lid_module.LID_CLEARANCE)
    pilot_x = outer_x - 2.0 * (gf.lip_inner_offset(0.0) + lid_module.LID_CLEARANCE)
    pilot_y = outer_y - 2.0 * (gf.lip_inner_offset(0.0) + lid_module.LID_CLEARANCE)
    neck_x = outer_x - 2.0 * gf.lip_inner_offset(0.0)
    neck_y = outer_y - 2.0 * gf.lip_inner_offset(0.0)

    box = shape.BoundBox
    return {
        "volume_mm3": harness.round6(shape.Volume),
        "area_mm2": harness.round6(shape.Area),
        "faces": len(shape.Faces),
        "edges": len(shape.Edges),
        "solids": len(shape.Solids),
        "shells": len(shape.Shells),
        "is_valid": bool(shape.isValid()),
        "is_closed": bool(len(shape.Shells) == 1 and shape.Shells[0].isClosed()),
        "bbox": {
            "x_mm": harness.round6(box.XLength),
            "y_mm": harness.round6(box.YLength),
            "z_mm": harness.round6(box.ZLength),
            "z_min_mm": harness.round6(box.ZMin),
            "z_max_mm": harness.round6(box.ZMax),
        },
        "expected": {
            "z_top_mm": harness.round6(z_top),
            "z_plate_bottom_mm": harness.round6(z_plate_bottom),
            "z_skirt_bottom_mm": harness.round6(z_skirt_bottom),
            "plate_x_mm": harness.round6(plate_x),
            "plate_y_mm": harness.round6(plate_y),
            "pilot_x_mm": harness.round6(pilot_x),
            "pilot_y_mm": harness.round6(pilot_y),
            "neck_x_mm": harness.round6(neck_x),
            "neck_y_mm": harness.round6(neck_y),
            "hole_centre_y_mm": harness.round6(
                outer_y / 2.0 - lid_module.FINGER_HOLE_EDGE_DISTANCE
            ),
        },
        "roundover_applied": bool(lid_module.top_roundover_applied(shape, height_u)),
        "top_face_width_mm": _top_face_width(shape, z_top),
        "sections": {
            "plate_above_bottom": _section(shape, z_plate_bottom + 0.05),
            "below_plate": _section(shape, z_plate_bottom - 0.05),
            "plate_mid": _section(shape, z_top - 1.0),
            "collar": _section(shape, z_lip + 1.0),
            "frustum_mid": _section(shape, z_lip + 0.35),
            "pilot": _section(shape, z_lip - 0.2),
            "chamfer_low": _section(shape, z_skirt_bottom + 0.05),
            "chamfer_high": _section(shape, z_skirt_bottom + 0.55),
        },
        "circles": _circles(shape),
    }


def _measure_body(shell_module, gf, units_x, units_y, height_u) -> dict[str, object]:
    """The mating body's lip surfaces at the lid's measurement heights."""
    body = shell_module.build_body(
        shell_module.BodySpec(units_x, units_y, height_u)
    ).shape
    total_h = height_u * shell_module.HEIGHT_UNIT
    z_lip = total_h - gf.LIP_HEIGHT
    return {
        "solids": len(body.Solids),
        "vertical_section": _section(body, z_lip + 1.0),
        "frustum_mid": _section(body, z_lip + 0.35),
        "neck": _section(body, z_lip - 0.2),
        "lip_top": _section(body, total_h - 0.05),
    }


def _measure_stack(
    gf, shell_module, lid_module, units_x, units_y, height_u
) -> dict[str, object]:
    """Seat a base foot in the pinned lip void and measure the clearances.

    The design stack places the next body's base bottom at the lip top plane
    (the nominal pitch, base bottom at ``height_u * 7``), 1.0 mm above the lid
    top. The model's contact seat is deeper: the base's bottom chamfer rests
    on the lip's upper ramp and its top chamfer on the lip's top chamfer,
    with the base bottom 0.35 mm below the lip bottom. Both positions are
    measured here:

    * the nominal position (``next_body_base_*``): the 1.0 mm gap that keeps
      the lid out of the stack load path at the design pitch;
    * the contact seat (``seat_*``, bisected on the built body): the seated
      base's solid underside against the lid's plate top.

    The base is solid (ADR-0013), so at the seat the lid's plate top is above
    the base's underside (``lid_top_clearance_mm`` is negative) and the base's
    wall material at the plate's mid height passes the plate's edge: with the
    lid installed the base cannot reach the lip seat, so a stacked body rests
    on the lid. That is the documented finding for the human checkpoint
    (#14); the architecture is deliberately unchanged.
    """
    import FreeCAD as App

    total_h = height_u * shell_module.HEIGHT_UNIT
    z_lip = total_h - gf.LIP_HEIGHT

    # The next body's base: the pinned solid foot, centred on one cell of the
    # body under test.
    cell_x = (0 - (units_x - 1) / 2.0) * gf.GRID - gf.GRID / 2.0
    cell_y = (0 - (units_y - 1) / 2.0) * gf.GRID - gf.GRID / 2.0
    centre_x = cell_x + gf.GRID / 2.0
    centre_y = cell_y + gf.GRID / 2.0
    base = gf.make_base_foot(cell_x, cell_y)

    body = shell_module.build_body(
        shell_module.BodySpec(units_x, units_y, height_u)
    ).shape
    lid = lid_module.build_lid(lid_module.BodySpec(units_x, units_y, height_u))
    lid_top = lid.BoundBox.ZMax

    nominal = base.translated(App.Vector(0.0, 0.0, total_h))
    out: dict[str, object] = {
        "next_body_base_z_min_mm": harness.round6(nominal.BoundBox.ZMin),
        "next_body_base_z_max_mm": harness.round6(nominal.BoundBox.ZMax),
        "pitch_mm": harness.round6(total_h),
        "lid_top_z_mm": harness.round6(lid_top),
    }

    def lip_intersection(z0: float) -> float:
        moved = base.translated(App.Vector(0.0, 0.0, z0))
        common = body.common(moved)
        return float(common.Volume) if not common.isNull() else 0.0

    # Bisect the deepest position with no lip intersection: the contact seat.
    low, high = z_lip - 1.0, z_lip + 0.5
    for _ in range(30):
        middle = (low + high) / 2.0
        if lip_intersection(middle) > 1e-9:
            low = middle
        else:
            high = middle
    seat = high
    out["seat_z0_mm"] = harness.round6(seat)
    out["seat_rel_lip_bottom_mm"] = harness.round6(seat - z_lip)
    out["seat_intersection_mm3"] = harness.round6(abs(lip_intersection(seat)))
    out["seat_contact_below_mm3"] = harness.round6(lip_intersection(seat - 0.05))

    seated = base.translated(App.Vector(0.0, 0.0, seat))
    common = seated.common(lid)
    out["base_lid_intersection_mm3"] = harness.round6(
        float(common.Volume) if not common.isNull() else 0.0
    )

    # The seated base is solid: the lowest material at the cell centre is its
    # flat underside. A negative ``lid_top_clearance_mm`` means the lid's top
    # face is above the base's underside, i.e. the base would rest on the lid.
    underside = harness.bisect_z(
        seated, centre_x, centre_y, seat - 0.2, seat + gf.BASE_HEIGHT
    )
    out["base_underside_z_mm"] = harness.round6(underside)
    out["lid_top_clearance_mm"] = harness.round6(underside - lid_top)

    # The base's outer wall against the lid's widest surface (plate edge /
    # skirt collar) at the plate's mid height, both measured from the cell
    # centre. Since the base is solid, material outside the base's wall line
    # does not exist, so a positive overlap means the plate's edge is buried
    # in the base's wall.
    probe_z = lid_top - 1.0
    base_radius = (
        harness.bisect_x(seated, centre_y, probe_z, centre_x, centre_x + 25.0)
        - centre_x
    )
    plate_radius = centre_x - harness.bisect_x(
        lid, centre_y, probe_z, centre_x, centre_x - 25.0
    )
    out["base_wall_radius_mm"] = harness.round6(base_radius)
    out["plate_edge_radius_mm"] = harness.round6(plate_radius)
    out["plate_edge_overlap_mm"] = harness.round6(base_radius - plate_radius)
    return out


def _measure_basic(lid_module, units_x, units_y, height_u) -> dict[str, object]:
    """Volume and bbox only, for the square 1x1 parity pins."""
    shape = lid_module.build_lid(lid_module.BodySpec(units_x, units_y, height_u))
    box = shape.BoundBox
    return {
        "volume_mm3": harness.round6(shape.Volume),
        "solids": len(shape.Solids),
        "is_valid": bool(shape.isValid()),
        "bbox": {
            "x_mm": harness.round6(box.XLength),
            "y_mm": harness.round6(box.YLength),
            "z_min_mm": harness.round6(box.ZMin),
            "z_max_mm": harness.round6(box.ZMax),
        },
    }


def main() -> int:
    import cad.gridfinity as gf
    import cad.lid as lid_module
    import cad.shell as shell_module

    report: dict[str, object] = {
        "module_imported": True,
        "constants": {
            "LID_PLATE": harness.round6(lid_module.LID_PLATE),
            "LID_SKIRT": harness.round6(lid_module.LID_SKIRT),
            "LID_CLEARANCE": harness.round6(lid_module.LID_CLEARANCE),
            "LID_CORNER_RADIUS": harness.round6(lid_module.LID_CORNER_RADIUS),
            "LID_SKIRT_CHAMFER": harness.round6(lid_module.LID_SKIRT_CHAMFER),
            "LID_TOP_ROUNDOVER": harness.round6(lid_module.LID_TOP_ROUNDOVER),
            "FINGER_HOLE_DIAMETER": harness.round6(lid_module.FINGER_HOLE_DIAMETER),
            "FINGER_HOLE_EDGE_DISTANCE": harness.round6(lid_module.FINGER_HOLE_EDGE_DISTANCE),
            "LID_RIB_PROUD": harness.round6(lid_module.LID_RIB_PROUD),
            "LID_RIB_LENGTH": harness.round6(lid_module.LID_RIB_LENGTH),
            "LID_RIB_HEIGHT": harness.round6(lid_module.LID_RIB_HEIGHT),
            "SKIRT_INTRUSION": harness.round6(lid_module.SKIRT_INTRUSION),
            "LIP_VERTICAL_OFFSET": harness.round6(lid_module.LIP_VERTICAL_OFFSET),
            "OUTER_RADIUS": harness.round6(gf.OUTER_RADIUS),
            "LIP_HEIGHT": harness.round6(gf.LIP_HEIGHT),
        },
        "lid_plain": _measure_lid(lid_module, gf, UNITS_X, UNITS_Y, HEIGHT_U, ribs=False),
        "lid_ribs": _measure_lid(lid_module, gf, UNITS_X, UNITS_Y, HEIGHT_U, ribs=True),
        "lid_1x1": _measure_basic(lid_module, 1, 1, 6),
        "lid_1x1_3u": _measure_basic(lid_module, 1, 1, 3),
        "body": _measure_body(shell_module, gf, UNITS_X, UNITS_Y, HEIGHT_U),
        "stack": _measure_stack(gf, shell_module, lid_module, UNITS_X, UNITS_Y, HEIGHT_U),
    }

    harness.emit(report)
    return 0


harness.run(main)
