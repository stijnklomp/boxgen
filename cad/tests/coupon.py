"""Headless coupon measurements (ticket #10, calibration section 2).

Run under the pinned launcher:

    freecadcmd cad/tests/coupon.py

The script builds the calibration coupon through the normal master path --
``cad.shell.build_body`` plus ``cad.dividers.build_coupon_divider`` for the
body and ``cad.lid.build_lid`` for the ribbed lid -- and measures every
number calibration section 2 fixes:

* body: outer 41.5 x 41.5 x 46.4 mm, one valid closed solid;
* T-divider: 2.0 mm thick, fused to the floor and the -y wall, 20 mm long
  from the wall's inner face, top at 32.0 mm (10 mm below the 42.0 lip
  bottom), 0.4 mm top chamfer; the thin-slab slice areas at z = 20 and
  z = 33 show the divider material and its absence above the top;
* junctions: the 2.0 mm floor and wall coves taper like the tree dividers'
  (material probes sit on the expected quarter-round boundary); the wall
  columns stop at the 0.4 mm top chamfer's lower edge (31.6 mm on the 6U
  coupon), so no corner stands above the chamfered plate top;
* lid: the ribbed 1 x 1 x 6U lid is 37.1 x 37.1 mm spanning z 41.0..45.0,
  with the 20 mm finger hole centred 14 mm from the +y edge and the ribs
  adding exactly 0.30 mm to the pilot width.

The script prints one marker line:

    BOXGEN_JSON:{...}

``tests/coupon.rs`` parses that line and asserts the calibration numbers.
``freecadcmd`` prints its banner to stdout, discards unflushed buffers when
the script returns, and exits 0 on an unhandled exception, so the marker
print flushes and the script exits non-zero on any failure.
"""

from __future__ import annotations

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(os.path.dirname(HERE))
if REPO_ROOT not in sys.path:
    sys.path.insert(0, REPO_ROOT)

from cad.tests import _harness as harness

#: The calibration coupon's fixed geometry (calibration section 2).
UNITS_X = 1
UNITS_Y = 1
HEIGHT_U = 6


def _section(shape, z: float, thickness: float = 0.02) -> dict[str, object]:
    """Slice ``shape`` with a thin slab and measure the material section.

    ``area_mm2`` is the slab volume over its thickness; ``wires`` lists the
    bounding boxes of the largest planar face's wires, smallest first, so a
    ring (body wall, lid skirt) yields its inner opening and outer face.
    """
    slab = harness.slab_section(shape, z, thickness)
    if slab is None:
        return {"z": harness.round6(z), "empty": True}
    return {
        "z": slab["z"],
        "area_mm2": slab["area_mm2"],
        "wires": [
            harness.wire_box(wire)
            for wire in sorted(
                slab["wires"],
                key=lambda wire: (wire.BoundBox.XLength, wire.BoundBox.YLength),
            )
        ],
    }


def _measure_body(dividers, gf, shell) -> dict[str, object]:
    """Build the coupon body and measure the T-divider and its junctions."""
    spec = shell.BodySpec(UNITS_X, UNITS_Y, HEIGHT_U)
    fused = dividers.build_coupon_divider(shell.build_body(spec).shape, spec)
    z_floor = shell.FLOOR_TOP
    top = z_floor + dividers.divider_height(HEIGHT_U)
    wall_face = -(gf.outer_width(UNITS_Y) - 2.0 * shell.WALL) / 2.0
    shells = fused.Shells

    out: dict[str, object] = {
        "volume_mm3": harness.round6(fused.Volume),
        "area_mm2": harness.round6(fused.Area),
        "faces": len(fused.Faces),
        "solids": len(fused.Solids),
        "shells": len(shells),
        "is_valid": bool(fused.isValid()),
        "is_closed": bool(len(shells) == 1 and shells[0].isClosed()),
        "bbox": {
            "x_mm": harness.round6(fused.BoundBox.XLength),
            "y_mm": harness.round6(fused.BoundBox.YLength),
            "z_mm": harness.round6(fused.BoundBox.ZLength),
            "z_min_mm": harness.round6(fused.BoundBox.ZMin),
            "z_max_mm": harness.round6(fused.BoundBox.ZMax),
        },
        "z_floor_mm": harness.round6(z_floor),
        "divider_top_mm": harness.round6(top),
        "wall_face_mm": harness.round6(wall_face),
        # The plate: centred faces, free end, and the top found by bisection.
        "near_face_mm": harness.round6(harness.bisect_x(fused, -10.0, 20.0, 0.0, -3.0)),
        "far_face_mm": harness.round6(harness.bisect_x(fused, -10.0, 20.0, 0.0, 3.0)),
        "free_end_mm": harness.round6(harness.bisect_y(fused, 0.0, 20.0, 0.0, 3.0)),
        "top_probe_mm": harness.round6(
            harness.measure_top(fused, 0.0, -10.0, z_floor + 0.5, 42.0)
        ),
        # Slices: the divider material is present at z = 20 and gone at 29.
        # The area difference is the 2 x 20 mm plate plus the two quarter-round
        # wall columns, 2 * (4 - pi).
        "material_at_z20": harness.point_is_material(fused, 0.0, -10.0, 20.0),
        "material_at_z33": harness.point_is_material(fused, 0.0, -10.0, 33.0),
        "section_z20": _section(fused, 20.0),
        "section_z33": _section(fused, 33.0),
        # The 2.0 mm floor cove taper: inset 1.3755 at 0.1 above the floor,
        # 0.4388 at 0.75, gone at 1.95.
        "cove_floor_material_0_1": harness.point_is_material(
            fused, -2.0, -10.0, z_floor + 0.1
        ),
        "cove_floor_void_0_1": not harness.point_is_material(
            fused, -2.5, -10.0, z_floor + 0.1
        ),
        "cove_floor_material_0_75": harness.point_is_material(
            fused, -1.35, -10.0, z_floor + 0.75
        ),
        "cove_floor_void_0_75": not harness.point_is_material(
            fused, -1.55, -10.0, z_floor + 0.75
        ),
        "cove_floor_void_at_top": not harness.point_is_material(
            fused, -1.1, -10.0, z_floor + 1.95
        ),
        "cove_floor_material_0_1_far": harness.point_is_material(
            fused, 2.0, -10.0, z_floor + 0.1
        ),
        "cove_floor_void_0_1_far": not harness.point_is_material(
            fused, 2.5, -10.0, z_floor + 0.1
        ),
        # The rail stops at the plate's free end: no material beside the
        # plate 0.35 mm past it.
        "cove_floor_void_past_free_end": not harness.point_is_material(
            fused, -1.5, 1.6, z_floor + 0.1
        ),
        # The 2.0 mm wall cove columns at both plate faces.
        "cove_wall_material": harness.point_is_material(fused, -1.1, -18.6, 10.0),
        "cove_wall_void": not harness.point_is_material(fused, -2.2, -17.9, 10.0),
        "cove_wall_material_far": harness.point_is_material(fused, 1.1, -18.6, 10.0),
        "cove_wall_void_far": not harness.point_is_material(fused, 2.2, -17.9, 10.0),
        # The wall columns stop at the 0.4 mm top chamfer's lower edge, so
        # their top is 31.6 mm rather than the plate's 32.0 mm.
        "cove_wall_top_mm": harness.round6(
            harness.measure_top(fused, -1.1, -18.6, z_floor + 0.5, 42.0)
        ),
        # The 0.4 mm top chamfer: 0.3 clear per side 0.1 below the top, flush
        # 0.5 below, and 0.3 short at the free end.
        "chamfer_near_mm": harness.round6(harness.bisect_x(fused, -10.0, top - 0.1, 0.0, -3.0)),
        "chamfer_far_mm": harness.round6(harness.bisect_x(fused, -10.0, top - 0.1, 0.0, 3.0)),
        "chamfer_free_end_mm": harness.round6(harness.bisect_y(fused, 0.0, top - 0.1, 0.0, 3.0)),
        "flush_near_mm": harness.round6(harness.bisect_x(fused, -10.0, top - 0.5, 0.0, -3.0)),
        "flush_far_mm": harness.round6(harness.bisect_x(fused, -10.0, top - 0.5, 0.0, 3.0)),
        "flush_free_end_mm": harness.round6(harness.bisect_y(fused, 0.0, top - 0.5, 0.0, 3.0)),
    }
    return out


def _measure_lid(lid_module, gf) -> dict[str, object]:
    """Build the coupon's ribbed lid and measure the standard features."""
    spec = lid_module.BodySpec(UNITS_X, UNITS_Y, HEIGHT_U)
    plain = lid_module.build_lid(spec)
    ribbed = lid_module.build_lid(spec, ribs=True)
    total_h = HEIGHT_U * lid_module.HEIGHT_UNIT + gf.LIP_HEIGHT
    z_lip = total_h - gf.LIP_HEIGHT
    z_top = z_lip + lid_module.SKIRT_INTRUSION + lid_module.LID_PLATE
    z_plate_bottom = z_top - lid_module.LID_PLATE
    shells = ribbed.Shells

    return {
        "solids": len(ribbed.Solids),
        "is_valid": bool(ribbed.isValid()),
        "is_closed": bool(len(shells) == 1 and shells[0].isClosed()),
        "bbox": {
            "x_mm": harness.round6(ribbed.BoundBox.XLength),
            "y_mm": harness.round6(ribbed.BoundBox.YLength),
            "z_min_mm": harness.round6(ribbed.BoundBox.ZMin),
            "z_max_mm": harness.round6(ribbed.BoundBox.ZMax),
        },
        "roundover_applied": bool(lid_module.top_roundover_applied(ribbed, HEIGHT_U)),
        "pilot_plain": _section(plain, z_lip - 0.2),
        "pilot_ribbed": _section(ribbed, z_lip - 0.2),
        "plate_section": _section(ribbed, z_plate_bottom + 0.5),
        "expected": {
            "z_lip_mm": harness.round6(z_lip),
            "z_top_mm": harness.round6(z_top),
            "z_plate_bottom_mm": harness.round6(z_plate_bottom),
            "hole_centre_y_mm": harness.round6(
                gf.outer_width(UNITS_Y) / 2.0 - lid_module.FINGER_HOLE_EDGE_DISTANCE
            ),
        },
    }


def main() -> int:
    import cad.dividers as dividers
    import cad.gridfinity as gf
    import cad.lid as lid_module
    import cad.shell as shell

    report: dict[str, object] = {
        "module_imported": True,
        "constants": {
            "COUPON_DIVIDER_LENGTH": harness.round6(dividers.COUPON_DIVIDER_LENGTH),
            "COUPON_DIVIDER_WALL": dividers.COUPON_DIVIDER_WALL,
            "DIVIDER_THICKNESS": harness.round6(dividers.DIVIDER_THICKNESS),
            "DIVIDER_TOP_CHAMFER": harness.round6(dividers.DIVIDER_TOP_CHAMFER),
            "JUNCTION_FILLET": harness.round6(dividers.JUNCTION_FILLET),
            "OUTER_WIDTH_MM": harness.round6(gf.outer_width(UNITS_X)),
            "HEIGHT_MM": harness.round6(HEIGHT_U * shell.HEIGHT_UNIT + gf.LIP_HEIGHT),
            "DIVIDER_GAP_MM": harness.round6(dividers.DIVIDER_GAP),
        },
        "body": _measure_body(dividers, gf, shell),
        "lid": _measure_lid(lid_module, gf),
        # The thin-slab slice areas prove the divider material is inside the
        # 20 mm top (z = 20) and gone above it (z = 33); the difference is the
        # 2 x 20 mm plate plus the two quarter-round wall columns.
        "expected_slice_area_difference_mm2": harness.round6(
            2.0 * dividers.COUPON_DIVIDER_LENGTH + 2.0 * (4.0 - math.pi)
        ),
    }

    harness.emit(report)
    return 0


harness.run(main)
