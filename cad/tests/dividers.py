"""Headless divider measurements for the SYS-1.0 division tree (ticket #8).

Run under the pinned launcher:

    freecadcmd cad/tests/dividers.py

The script covers the ticket #8 contract (standard section 4.3, generator
contract section 4):

* ``cad/dividers.py`` resolves the division tree to absolute divider
  rectangles (2.0 mm thick, spanning the whole parent rectangle), fuses the
  plates, the 0.4 mm top chamfers, the 2.0 mm floor and wall junction coves,
  and returns the body as one valid, closed solid.
* Divider height above the floor: interior depth minus 2.0 mm on 3U bodies,
  minus 10.0 mm on 6U and taller (measured tops 15.0 / 28.0 / 49.0 / 70.0).
* The contract's 4 x 4 golden tree (x at 80; left child y at 50; right child
  x at 40) is measured by slicing the built body: at z = 20 the four
  compartment wires pin the three dividers' near faces and full spans; at
  z = 30 the dividers are gone.
* Junction coves: slicing just above the floor shows the 2.0 mm cove inset
  tapering from the face; corner junction and T junction probes are material
  where the sharp corner used to be, and void where the cove does not reach.
* Top chamfer: 0.1 mm below the divider top the section is 0.3 mm clear of
  the face; below the chamfer band it is flush. The cove columns stop at the
  chamfer's lower edge (0.4 mm below the top), so no junction corner stands
  above the chamfered plate.

The script prints one marker line:

    BOXGEN_JSON:{...}

``tests/dividers.rs`` parses that line and asserts the SYS-1.0 numbers.
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

#: The contract's golden 4 x 4 tree (generator contract section 4): x at 80,
#: the left child y at 50, the right child x at 40.
GOLDEN_TREE = {
    "axis": "x",
    "at_mm": 80,
    "near": {
        "axis": "y",
        "at_mm": 50,
        "near": {"compartment": "A"},
        "far": {"compartment": "B"},
    },
    "far": {
        "axis": "x",
        "at_mm": 40,
        "near": {"compartment": "C"},
        "far": {"compartment": "D"},
    },
}

#: A synthetic corner-end tree: the y divider's far face (78.25 + 2 = 80.25)
#: lands on the inner corner arc of a 4 x 4 body. Contract validation rejects
#: such a tree (the far child would be 1.25 mm deep, far below the 12 mm
#: minimum), so no valid spec can produce it; the tree is built directly to
#: prove the corner handling in ``build_dividers``. See ``cad/dividers.py``.
CORNER_TREE = {
    "axis": "y",
    "at_mm": 160,
    "near": {"compartment": "A"},
    "far": {"compartment": "B"},
}

#: A synthetic tree whose divider cross position (79.25) lands on the corner
#: arc: an x divider with side faces at x = 78.25 and 80.25.
CORNER_CROSS_TREE = {
    "axis": "x",
    "at_mm": 160,
    "near": {"compartment": "A"},
    "far": {"compartment": "B"},
}

#: 1 x 1 tree used for the height sweep: a single split at 18 mm.
HEIGHT_TREE = {
    "axis": "x",
    "at_mm": 18,
    "near": {"compartment": "A"},
    "far": {"compartment": "B"},
}

#: One split of a 4 x 4 body. The divider does not pass over the four centre
#: feet, so this tree pins the shell's floor slab: the slab spans the cell
#: grooves, so the centre feet stay one solid with the wall.
SINGLE_SPLIT_TREE = {
    "axis": "x",
    "at_mm": 80,
    "near": {"compartment": "A"},
    "far": {"compartment": "B"},
}


def _horizontal_section(shape, z: float) -> list[list[float]]:
    """Section wire bounding boxes at height ``z``, left-to-right."""
    import FreeCAD as App

    return sorted(
        (harness.wire_box(wire) for wire in shape.slice(App.Vector(0.0, 0.0, 1.0), z)),
        key=lambda box: (box[0], box[1]),
    )


def _compartments(shape, z: float) -> list[list[float]]:
    """The compartment openings (interior wires) at height ``z``.

    Wires smaller than 100 mm2 are dropped: above the divider tops the 0.4 mm
    chamfer leaves a 0.3 x 0.4 step beside each cove column, which is a
    separate section wire but not a compartment.
    """
    boxes = _horizontal_section(shape, z)
    if not boxes:
        return []
    outer = max(
        boxes,
        key=lambda box: (box[2] - box[0]) * (box[3] - box[1]),
    )
    return [
        box
        for box in boxes
        if box is not outer and (box[2] - box[0]) * (box[3] - box[1]) >= 100.0
    ]


def _section_vertices(shape, z: float) -> list[tuple[float, float]]:
    """Every wire vertex of the section at height ``z``."""
    import FreeCAD as App

    points = []
    for wire in shape.slice(App.Vector(0.0, 0.0, 1.0), z):
        for edge in wire.Edges:
            for vertex in edge.Vertexes:
                points.append((harness.round6(vertex.Point.x), harness.round6(vertex.Point.y)))
    return points


def _section_circles(shape, z: float) -> list[dict[str, object]]:
    """Circular edges of the section at height ``z`` with centre and radius."""
    import FreeCAD as App

    circles = []
    for wire in shape.slice(App.Vector(0.0, 0.0, 1.0), z):
        for edge in wire.Edges:
            try:
                curve = edge.Curve
            except Exception:  # noqa: BLE001 - boolean edges can be degenerate
                continue
            if curve.TypeId == "Part::GeomCircle":
                circles.append(
                    {
                        "radius_mm": harness.round6(curve.Radius),
                        "centre_mm": [
                            harness.round6(curve.Center.x),
                            harness.round6(curve.Center.y),
                        ],
                    },
                )
    return circles


def _arc_tip(arc_centre: float, radius: float, offset: float) -> float:
    """Second coordinate of the arc point offset ``offset`` from its centre."""
    return arc_centre + (radius * radius - offset * offset) ** 0.5


def _describe(shape, z_floor: float) -> dict[str, object]:
    shells = shape.Shells
    return {
        "volume_mm3": harness.round6(shape.Volume),
        "solids": len(shape.Solids),
        "is_valid": bool(shape.isValid()),
        "is_closed": bool(len(shells) == 1 and shells[0].isClosed()),
        "z_floor_mm": harness.round6(z_floor),
    }


def _golden_body(module, shell):
    body = shell.build_body(shell.BodySpec(4, 4, 6)).shape
    return module.build_dividers(body, shell.BodySpec(4, 4, 6), GOLDEN_TREE)


def _measure_golden(module, shell, gf) -> dict[str, object]:
    """Build the 4 x 4 x 6U golden tree and measure it by slicing."""
    fused = _golden_body(module, shell)
    out = _describe(fused, shell.FLOOR_TOP)

    top = shell.FLOOR_TOP + module.divider_height(6)
    out["divider_top_mm"] = harness.round6(top)
    out["section_z20"] = _compartments(fused, 20.0)
    out["section_z30"] = _compartments(fused, 30.0)
    out["section_z27_9"] = _compartments(fused, top - 0.1)
    out["section_z27_5"] = _compartments(fused, top - 0.5)

    # Junction coves: the void inset from a face at height z is
    # 2.0 - sqrt(4 - (z - floor - 2.0)^2), 2.0 at the floor tapering to 0.
    floor = shell.FLOOR_TOP
    out["section_floor_0_25"] = _compartments(fused, floor + 0.25)
    out["section_floor_1_95"] = _compartments(fused, floor + 1.95)
    out["section_floor_2_75"] = _compartments(fused, floor + 2.75)

    # Junction probes. Compartment A's far corner is the T junction of the y
    # divider on the x divider at (-1.75, -31.75); the root divider's end at
    # the -y wall is at (-1.75, -81.75).
    out["tee_material"] = harness.point_is_material(fused, -2.1, -32.1, floor + 0.6)
    out["tee_void"] = harness.point_is_material(fused, -6.0, -36.0, floor + 0.6)
    out["wall_material"] = harness.point_is_material(fused, -2.1, -81.4, floor + 0.6)
    out["wall_void"] = harness.point_is_material(fused, -5.0, -80.4, floor + 0.6)

    # The cove columns stop at the 0.4 mm top chamfer's lower edge, so their
    # top is 27.6 mm on a 6U body rather than the plate's 28.0 mm. The raw
    # section just under the top must contain only the body outline and the
    # four compartments: any island beside a divider face would be a corner
    # standing above the chamfer.
    out["wall_corner_top_mm"] = harness.round6(
        harness.measure_top(fused, -2.1, -81.4, floor + 1.0, top + 1.0)
    )
    out["tee_corner_top_mm"] = harness.round6(
        harness.measure_top(fused, -2.1, -32.1, floor + 1.0, top + 1.0)
    )
    out["section_top_minus_0_01"] = _horizontal_section(fused, top - 0.01)
    return out


def _measure_corner(module, shell, gf) -> dict[str, object]:
    """Force the corner-end case with a synthetic tree (see CORNER_TREE)."""
    fused = module.build_dividers(
        shell.build_body(shell.BodySpec(4, 4, 6)).shape,
        shell.BodySpec(4, 4, 6),
        CORNER_TREE,
    )
    out = _describe(fused, shell.FLOOR_TOP)
    inner_half = (gf.outer_width(4) - 2.0 * 2.0) / 2.0

    # Far face y = 80.25 meets the +x inner corner arc; the notch tip is the
    # plane/arc intersection at (arc, 80.25). The 2.0 mm cove at the
    # projected wall face must bury it.
    face = -inner_half + 160.0 + 2.0
    tip = (_arc_tip(inner_half - 1.75, 1.75, face - (inner_half - 1.75)), face)
    out["notch_tip_mm"] = [harness.round6(tip[0]), harness.round6(tip[1])]
    out["notch_tip_material"] = harness.point_is_material(fused, tip[0], tip[1], 10.0)
    out["notch_material"] = harness.point_is_material(fused, tip[0] - 0.3, tip[1] + 0.25, 10.0)
    out["notch_outside_is_void"] = not harness.point_is_material(
        fused, tip[0] - 1.22, tip[1] + 1.0, 10.0
    )

    out["cove_circles"] = [
        circle
        for circle in _section_circles(fused, 10.0)
        if abs(circle["radius_mm"] - module.JUNCTION_FILLET) < 1e-6
    ]
    vertices = _section_vertices(fused, 10.0)
    out["nearest_vertex_to_tip_mm"] = harness.round6(
        min(((vx - tip[0]) ** 2 + (vy - tip[1]) ** 2) ** 0.5 for vx, vy in vertices)
    )
    out["section_z10"] = _compartments(fused, 10.0)
    return out


def _measure_corner_cross(module, shell, gf) -> dict[str, object]:
    """Force a divider cross position on the corner arc (CORNER_CROSS_TREE)."""
    fused = module.build_dividers(
        shell.build_body(shell.BodySpec(4, 4, 6)).shape,
        shell.BodySpec(4, 4, 6),
        CORNER_CROSS_TREE,
    )
    out = _describe(fused, shell.FLOOR_TOP)
    inner_half = (gf.outer_width(4) - 2.0 * 2.0) / 2.0

    # The divider side face x = 80.25 meets the +y inner corner arc; the
    # notch tip is below the arc at (80.25, arc).
    face = -inner_half + 160.0 + 2.0
    tip = (face, _arc_tip(inner_half - 1.75, 1.75, face - (inner_half - 1.75)))
    out["notch_tip_mm"] = [harness.round6(tip[0]), harness.round6(tip[1])]
    out["notch_tip_material"] = harness.point_is_material(fused, tip[0], tip[1], 10.0)
    out["notch_material"] = harness.point_is_material(fused, tip[0] - 0.25, tip[1] - 0.3, 10.0)
    out["section_z10"] = _compartments(fused, 10.0)
    out["cove_circles"] = [
        circle
        for circle in _section_circles(fused, 10.0)
        if abs(circle["radius_mm"] - module.JUNCTION_FILLET) < 1e-6
    ]
    return out


def _measure_heights(module, shell, gf) -> dict[str, object]:
    """Measure the divider top for every legal height on a 1 x 1 body."""
    out: dict[str, object] = {}
    for height_u in (3, 6, 9, 12):
        spec = shell.BodySpec(1, 1, height_u)
        body = shell.build_body(spec).shape
        fused = module.build_dividers(body, spec, HEIGHT_TREE)
        top = harness.measure_top(
            fused,
            0.25,
            0.0,
            shell.FLOOR_TOP + 0.5,
            height_u * 7.0 - gf.LIP_HEIGHT,
        )
        out[str(height_u)] = {
            "height_above_floor_mm": harness.round6(module.divider_height(height_u)),
            "top_mm": harness.round6(top),
            "solids": len(fused.Solids),
            "is_closed": bool(
                len(fused.Shells) == 1 and fused.Shells[0].isClosed()
            ),
        }
    return out


def _measure_single_split(module, shell, gf) -> dict[str, object]:
    """One 4 x 4 split whose divider misses the centre feet (floor slab)."""
    fused = module.build_dividers(
        shell.build_body(shell.BodySpec(4, 4, 6)).shape,
        shell.BodySpec(4, 4, 6),
        SINGLE_SPLIT_TREE,
    )
    out = _describe(fused, shell.FLOOR_TOP)
    out["section_z20"] = _compartments(fused, 20.0)
    return out


def main() -> int:
    import cad.dividers as dividers
    import cad.gridfinity as gf
    import cad.shell as shell

    report: dict[str, object] = {
        "module_imported": True,
        "constants": {
            "DIVIDER_THICKNESS": harness.round6(dividers.DIVIDER_THICKNESS),
            "DIVIDER_TOP_CHAMFER": harness.round6(dividers.DIVIDER_TOP_CHAMFER),
            "DIVIDER_GAP_3U": harness.round6(dividers.DIVIDER_GAP_3U),
            "DIVIDER_GAP": harness.round6(dividers.DIVIDER_GAP),
            "JUNCTION_FILLET": harness.round6(dividers.JUNCTION_FILLET),
        },
    }

    report["plan_golden"] = [
        divider.to_dict()
        for divider in dividers.divider_plan(shell.BodySpec(4, 4, 6), GOLDEN_TREE)
    ]
    report["plan_corner"] = [
        divider.to_dict()
        for divider in dividers.divider_plan(shell.BodySpec(4, 4, 6), CORNER_TREE)
    ]
    report["plan_single_leaf"] = [
        divider.to_dict()
        for divider in dividers.divider_plan(
            shell.BodySpec(4, 4, 6), {"compartment": "x"}
        )
    ]

    # A leaf tree must not add any material.
    plain = shell.build_body(shell.BodySpec(4, 4, 6)).shape
    leaf = dividers.build_dividers(
        plain, shell.BodySpec(4, 4, 6), {"compartment": "x"}
    )
    report["plain_body_volume_mm3"] = harness.round6(plain.Volume)
    report["leaf_tree_volume_mm3"] = harness.round6(leaf.Volume)
    report["leaf_tree_solids"] = len(leaf.Solids)

    report["golden"] = _measure_golden(dividers, shell, gf)
    report["heights"] = _measure_heights(dividers, shell, gf)
    report["corner"] = _measure_corner(dividers, shell, gf)
    report["corner_cross"] = _measure_corner_cross(dividers, shell, gf)
    report["single_split"] = _measure_single_split(dividers, shell, gf)

    harness.emit(report)
    return 0


harness.run(main)
