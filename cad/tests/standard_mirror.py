"""The Rust mirror of the fixed architecture, as the serve panel shows it.

Run under the pinned launcher:

    freecadcmd cad/tests/standard_mirror.py

Imports the master modules and emits every constant that the Rust side
mirrors (``src/tree.rs``, and the ``GET /api/standard`` panel built from it).
Each mirrored Rust constant names its Python source in its comment;
``tests/standard_mirror.rs`` parses this marker and asserts the two sides
still agree, so the panel can never drift from the geometry the generated
scripts build (ADR-0002: the UI owns no geometry and no rules).
"""

from __future__ import annotations

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CAD_DIR = os.path.dirname(HERE)
REPO_ROOT = os.path.dirname(CAD_DIR)
if REPO_ROOT not in sys.path:
    sys.path.insert(0, REPO_ROOT)

from cad import build, dividers, gridfinity, lid, shell
from cad.tests import _harness as harness


def main() -> int:
    report = {
        "gridfinity": {
            "grid_mm": gridfinity.GRID,
            "clearance_per_side_mm": gridfinity.CLEARANCE,
            "base_height_mm": gridfinity.BASE_HEIGHT,
            "lip_height_mm": gridfinity.LIP_HEIGHT,
            "outer_radius_mm": gridfinity.OUTER_RADIUS,
        },
        "shell": {
            "wall_mm": shell.WALL,
            "height_unit_mm": shell.HEIGHT_UNIT,
            "floor_mm": shell.FLOOR,
            "floor_top_mm": shell.FLOOR_TOP,
            "inner_radius_mm": shell.INNER_RADIUS,
            "floor_fillet_mm": shell.FLOOR_FILLET,
        },
        "dividers": {
            "thickness_mm": dividers.DIVIDER_THICKNESS,
            "top_chamfer_mm": dividers.DIVIDER_TOP_CHAMFER,
            "gap_3u_mm": dividers.DIVIDER_GAP_3U,
            "gap_mm": dividers.DIVIDER_GAP,
            "junction_fillet_mm": dividers.JUNCTION_FILLET,
        },
        "lid": {
            "plate_mm": lid.LID_PLATE,
            "skirt_mm": lid.LID_SKIRT,
            "clearance_per_side_mm": lid.LID_CLEARANCE,
            "corner_radius_mm": lid.LID_CORNER_RADIUS,
            "top_roundover_mm": lid.LID_TOP_ROUNDOVER,
            "skirt_chamfer_mm": lid.LID_SKIRT_CHAMFER,
            "finger_hole_diameter_mm": lid.FINGER_HOLE_DIAMETER,
            "finger_hole_edge_distance_mm": lid.FINGER_HOLE_EDGE_DISTANCE,
            "rib_protrusion_mm": lid.LID_RIB_PROUD,
        },
        "estimate": {
            "density_g_per_cm3": build.PETG_DENSITY_G_PER_CM3,
            "effective_flow_mm3_per_s": build.EFFECTIVE_FLOW_MM3_PER_S,
        },
    }
    harness.emit(report)
    return 0


harness.run(main)
