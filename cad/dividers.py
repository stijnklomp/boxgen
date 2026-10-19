"""Divider solids from the division tree (standard section 4.3, contract 4).

``build_dividers`` fuses one 2.0 mm plate per split into the body shell:
the plate spans its whole parent rectangle, so every divider connects to the
outer wall or to an ancestor divider through the tree and a floating divider
cannot occur. Absolute positions are resolved by walking the tree from the
body interior origin in centred coordinates; ``at_mm`` is the distance from
the parent rectangle's origin edge to the divider's near face, and the far
child starts 2.0 mm later. :func:`divider_plan` returns one :class:`Divider`
per split (axis, absolute near face, parent span, and the two
:class:`SpanEnd` kinds); the axis-specific plate, rail, and column helpers
are methods or dispatched through :func:`cove_rail` so the two axes share
one code path.

Divider height above the floor is interior depth minus 2.0 mm on 3U bodies
and minus 10.0 mm on 6U and taller (standard section 4.3), measured plate
tops 15.0 / 28.0 / 49.0 / 70.0 mm. The cove columns end 0.4 mm lower, at the
plate chamfer's lower edge.

Junction fillets: OCC cannot fillet a fused shell's divider junctions
(``makeFillet`` fails; see the FreeCAD API notes section 6), so the 2.0 mm
cove is built by boolean instead, exactly like the shell's floor cove:

* a horizontal **cove rail** per plate face (box minus quarter cylinder)
  fills the divider-to-floor junction;
* a vertical **cove column** at every span end fills the divider-to-wall and
  T junction corners, with the quarter cylinder's void quadrant pointing into
  the child compartment. The columns stop at the plate chamfer's lower edge
  (``top - DIVIDER_TOP_CHAMFER``), so no junction corner stands above the
  chamfered divider top.

Calibration coupon (ticket #10). :func:`build_coupon_divider` fuses the
coupon's single T-divider into a 1 x 1 x 6U shell: a 2.0 mm plate centred on
the interior's x = 0, running :data:`COUPON_DIVIDER_LENGTH` from the -y wall
into the interior, with its top at ``z_floor + divider_height(6) = 28.0``
(10 mm below the lip), the 0.4 mm top-edge chamfer, and the same 2.0 mm floor
rails and wall columns as a tree divider. The free end carries no column
(there is no wall to fillet against); the calibration's 20 mm weight foot
rests on the 20 mm top.

Corner ends. The body's inner vertical corners are 1.75 mm arcs, so a
divider face or side can in principle meet the wall on the corner arc rather
than on the straight run. Columns are then still placed on the *projected*
straight wall face: the 2.0 mm cove radius is larger than the 1.75 mm corner
radius, so the column's quarter-round swallows the whole acute notch between
the divider face and the arc, staying tangent to the face and to the
straight wall below the arc; a 1.0 mm cove would at worst touch the notch
tip, never carve it. No sharp internal corner and no approximated radius is
left behind. Contract validation makes the case unreachable in a valid spec
(a split's far child is at least 12 mm deep, so every divider face stays at
least 12 mm clear of its parent's edge, while the corner band starts only
1.75 mm from the wall); ``cad/tests/dividers.py`` forces it anyway with
synthetic trees so the handling cannot regress. The one documented bound of
the overlap construction: the 1.0 mm span extension of a plate or rail
reaches ``inner / 2 + 1.0`` (82.75 mm on a 4 x 4 body), whose corner stays
inside the 3.75 mm outer arc while the divider's cross face is at most
82.55 mm from the body centre (0.8 mm beyond the inner wall face) — the
12 mm rule leaves every legal face more than 12 mm clear.

Every fuse is checked for exactly one solid with positive volume, and the
finished body for exactly one closed shell.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum

import FreeCAD as App
import Part

from . import gridfinity as gf
from . import shell
from .geometry import GeometryError, require_one_solid

__all__ = [
    "COUPON_DIVIDER_LENGTH",
    "COUPON_DIVIDER_WALL",
    "DIVIDER_GAP",
    "DIVIDER_GAP_3U",
    "DIVIDER_THICKNESS",
    "DIVIDER_TOP_CHAMFER",
    "Divider",
    "END_OVERLAP",
    "JUNCTION_FILLET",
    "SpanEnd",
    "build_coupon_divider",
    "build_dividers",
    "cove_column",
    "cove_rail",
    "cove_rail_x",
    "cove_rail_y",
    "divider_height",
    "divider_plan",
    "span_end_kind",
]

#: Every divider is 2.0 mm thick (standard section 4).
DIVIDER_THICKNESS = 2.0

#: Divider top-edge chamfer (standard section 4.1).
DIVIDER_TOP_CHAMFER = 0.4

#: 3U divider gap below the lip: interior depth minus 2.0 mm (standard 4.3).
DIVIDER_GAP_3U = 2.0

#: 6U and taller divider gap: interior depth minus 10.0 mm (standard 4.3).
DIVIDER_GAP = 10.0

#: Divider-to-floor and divider-to-wall cove radius (standard section 4.1).
#: The same 2.0 mm radius as the shell's floor-to-wall fillet.
JUNCTION_FILLET = shell.FLOOR_FILLET

#: How far each plate end and rail extends into the wall or the ancestor
#: divider at a T junction, so the fuse overlaps instead of sharing a face.
END_OVERLAP = 1.0

#: Calibration coupon (ticket #10, calibration section 2): the T-divider runs
#: this far from the -y wall's inner face into the interior. The calibration's
#: 5 kg weight has a 20 mm foot that must rest on the top, so the plate is
#: exactly that long and centred on the interior's x = 0.
COUPON_DIVIDER_LENGTH = 20.0

#: The wall the coupon T-divider is fused to: the -y wall, opposite the lid's
#: +y finger-hole edge, so the weight test and the lift hole do not interact.
COUPON_DIVIDER_WALL = "-y"

#: Length tolerance for picking a plate's long top edges out of the chamfer
#: edge list (the 2.0 mm end edges are handled too; they sit inside the
#: wall).
_EDGE_LENGTH_TOLERANCE = 1.0


class SpanEnd(Enum):
    """How a divider span end meets the shell (see :func:`span_end_kind`)."""

    #: The end sits on the body's inner wall.
    WALL = "wall"
    #: The end sits on an ancestor divider's face.
    TEE = "tee"


@dataclass(frozen=True)
class Divider:
    """One resolved split: a 2.0 mm plate perpendicular to ``axis``.

    ``near_face_mm`` is the absolute (centred) coordinate of the face nearer
    the parent origin, on ``axis``; the plate material spans
    ``near_face_mm .. far_face_mm`` there and the whole parent rectangle
    ``span_lo_mm .. span_hi_mm`` on the other axis. ``lo_end`` and ``hi_end``
    classify how the two span ends meet the shell. ``axis: x`` is a plate
    perpendicular to X, ``axis: y`` one perpendicular to Y.
    """

    axis: str
    near_face_mm: float
    span_lo_mm: float
    span_hi_mm: float
    lo_end: SpanEnd
    hi_end: SpanEnd

    @property
    def far_face_mm(self) -> float:
        """Absolute coordinate of the face away from the parent origin."""
        return self.near_face_mm + DIVIDER_THICKNESS

    def to_dict(self) -> dict:
        """The report shape (standard section 4.3)."""
        return {
            "axis": self.axis,
            "near_face_mm": self.near_face_mm,
            "span_lo_mm": self.span_lo_mm,
            "span_hi_mm": self.span_hi_mm,
            "lo_end": self.lo_end.value,
            "hi_end": self.hi_end.value,
        }

    def plate_bounds(self) -> tuple[float, float, float, float]:
        """``(x0, y0, width, depth)`` of the plate, end overlaps included."""
        lo = self.span_lo_mm - END_OVERLAP
        hi = self.span_hi_mm + END_OVERLAP
        if self.axis == "x":
            return (self.near_face_mm, lo, DIVIDER_THICKNESS, hi - lo)
        return (lo, self.near_face_mm, hi - lo, DIVIDER_THICKNESS)

    def rail_faces(self) -> list[tuple[float, int]]:
        """``(face_mm, side)`` for the two floor rails: near face, far face."""
        return [(self.near_face_mm, -1), (self.far_face_mm, 1)]

    def junction_placements(self) -> list[tuple[float, float, int, int]]:
        """The four cove column placements for the two span ends.

        A wall end and a T end share the same placement: the column sits on
        the face at ``span_lo_mm`` / ``span_hi_mm`` -- the body's projected
        straight wall face for a ``wall`` end (see the module docstring for
        the corner-arc case), or an ancestor divider's face for a ``tee``
        end. The cove radius exceeds the 1.75 mm inner corner radius, so a
        junction that lands on the corner arc is covered too. Each tuple is
        ``(x_face, y_face, sx, sy)`` for :func:`cove_column`, with the void
        quadrant pointing into the child compartment: ``+1`` at the low span
        end, ``-1`` at the high end.
        """
        placements: list[tuple[float, float, int, int]] = []
        near_face = self.near_face_mm
        far_face = self.far_face_mm
        if self.axis == "x":
            for end, sy in ((self.span_lo_mm, 1), (self.span_hi_mm, -1)):
                placements.append((near_face, end, -1, sy))
                placements.append((far_face, end, 1, sy))
        else:
            for end, sx in ((self.span_lo_mm, 1), (self.span_hi_mm, -1)):
                placements.append((end, near_face, sx, -1))
                placements.append((end, far_face, sx, 1))
        return placements


def divider_height(height_u: int) -> float:
    """Divider height above the floor (standard section 4.3).

    3U bodies clear the lid skirt with interior depth minus 2.0 mm; 6U and
    taller leave a finger gap with interior depth minus 10.0 mm.
    """
    if height_u < 3:
        raise GeometryError(f"height must be at least 3U, got {height_u}U")
    gap = DIVIDER_GAP_3U if height_u == 3 else DIVIDER_GAP
    # Grouped as one subtraction so the f64 result is the clean decimal for
    # every accepted height (22.45, 9.45, ...).
    height = height_u * shell.HEIGHT_UNIT - (shell.FLOOR_TOP + gf.LIP_HEIGHT + gap)
    if height <= 0.0:
        raise GeometryError(f"{height_u}U leaves no divider height above the floor")
    return height


def span_end_kind(inner_half_mm: float, end_mm: float) -> SpanEnd:
    """Classify one divider span end: :attr:`SpanEnd.WALL` or :attr:`SpanEnd.TEE`.

    A ``wall`` end sits on the body's inner wall (``+-inner_half_mm``); a
    ``tee`` end sits on an ancestor divider's face, because the divider spans
    its parent rectangle and the parent's edge is either the wall or an
    ancestor's face.
    """
    if abs(abs(end_mm) - inner_half_mm) < 1e-6:
        return SpanEnd.WALL
    return SpanEnd.TEE


def divider_plan(spec: shell.BodySpec, tree: dict) -> list[Divider]:
    """Resolve a division tree to absolute :class:`Divider` plates.

    Returns one entry per split, nested dividers before their parent. A leaf
    node adds nothing.
    """
    inner_width = gf.outer_width(spec.units_x) - 2.0 * shell.WALL
    inner_depth = gf.outer_width(spec.units_y) - 2.0 * shell.WALL
    half_width = inner_width / 2.0
    half_depth = inner_depth / 2.0

    dividers: list[Divider] = []

    def walk(node, x0: float, y0: float, x1: float, y1: float) -> None:
        if not isinstance(node, dict) or "axis" not in node:
            return
        axis = node.get("axis")
        at_mm = float(node.get("at_mm", 0.0))
        if axis == "x":
            near_face = x0 + at_mm
            walk(node.get("near", {}), x0, y0, near_face, y1)
            walk(node.get("far", {}), near_face + DIVIDER_THICKNESS, y0, x1, y1)
            dividers.append(
                Divider(
                    axis="x",
                    near_face_mm=near_face,
                    span_lo_mm=y0,
                    span_hi_mm=y1,
                    lo_end=span_end_kind(half_depth, y0),
                    hi_end=span_end_kind(half_depth, y1),
                )
            )
        elif axis == "y":
            near_face = y0 + at_mm
            walk(node.get("near", {}), x0, y0, x1, near_face)
            walk(node.get("far", {}), x0, near_face + DIVIDER_THICKNESS, x1, y1)
            dividers.append(
                Divider(
                    axis="y",
                    near_face_mm=near_face,
                    span_lo_mm=x0,
                    span_hi_mm=x1,
                    lo_end=span_end_kind(half_width, x0),
                    hi_end=span_end_kind(half_width, x1),
                )
            )

    walk(tree, -half_width, -half_depth, half_width, half_depth)
    return dividers


def cove_rail_y(
    x_face: float,
    side: int,
    y0: float,
    y1: float,
    z0: float,
    radius: float = JUNCTION_FILLET,
) -> Part.Shape:
    """Cove along a divider face at ``x = x_face``, spanning ``y0..y1``.

    The returned solid (box minus quarter cylinder) adds exactly the
    quarter-round of a ``radius`` fillet against the face, from the floor at
    ``z0`` up. ``side`` selects the void side: ``-1`` for material at
    ``x > x_face``, ``+1`` for material at ``x < x_face``.
    """
    if side < 0:
        centre_x = x_face - radius
        box = Part.makeBox(
            radius, y1 - y0, radius, App.Vector(centre_x, y0, z0)
        )
        cylinder = Part.makeCylinder(
            radius,
            y1 - y0,
            App.Vector(centre_x, y0, z0 + radius),
            App.Vector(0, 1, 0),
        )
    else:
        centre_x = x_face + radius
        box = Part.makeBox(radius, y1 - y0, radius, App.Vector(x_face, y0, z0))
        cylinder = Part.makeCylinder(
            radius,
            y1 - y0,
            App.Vector(centre_x, y0, z0 + radius),
            App.Vector(0, 1, 0),
        )
    return box.cut(cylinder)


def cove_rail_x(
    y_face: float,
    side: int,
    x0: float,
    x1: float,
    z0: float,
    radius: float = JUNCTION_FILLET,
) -> Part.Shape:
    """Cove along a divider face at ``y = y_face``, spanning ``x0..x1``.

    ``side`` selects the void side like :func:`cove_rail_y`: ``-1`` for
    material at ``y > y_face``, ``+1`` for material at ``y < y_face``.
    """
    if side < 0:
        centre_y = y_face - radius
        box = Part.makeBox(x1 - x0, radius, radius, App.Vector(x0, centre_y, z0))
        cylinder = Part.makeCylinder(
            radius,
            x1 - x0,
            App.Vector(x0, centre_y, z0 + radius),
            App.Vector(1, 0, 0),
        )
    else:
        centre_y = y_face + radius
        box = Part.makeBox(x1 - x0, radius, radius, App.Vector(x0, y_face, z0))
        cylinder = Part.makeCylinder(
            radius,
            x1 - x0,
            App.Vector(x0, centre_y, z0 + radius),
            App.Vector(1, 0, 0),
        )
    return box.cut(cylinder)


def cove_rail(
    axis: str,
    face_mm: float,
    side: int,
    span_lo: float,
    span_hi: float,
    z0: float,
    radius: float = JUNCTION_FILLET,
) -> Part.Shape:
    """Cove along a divider face, dispatched on the divider's axis.

    ``axis`` is the axis the face is perpendicular to: an ``x`` divider's
    faces are planes of constant x spanning y, a ``y`` divider's faces are
    planes of constant y spanning x. ``side`` selects the void side exactly
    as in :func:`cove_rail_y`: ``-1`` for material at the far side of the
    face, ``+1`` for material at the near side.
    """
    if axis == "x":
        return cove_rail_y(face_mm, side, span_lo, span_hi, z0, radius)
    if axis == "y":
        return cove_rail_x(face_mm, side, span_lo, span_hi, z0, radius)
    raise GeometryError(f"unknown divider axis {axis!r}")


def cove_column(
    x_face: float,
    y_face: float,
    sx: int,
    sy: int,
    z0: float,
    z1: float,
    radius: float = JUNCTION_FILLET,
) -> Part.Shape:
    """Vertical cove at an internal corner ``(x_face, y_face)``.

    ``(sx, sy)`` point from each face into the void quadrant: the solid adds
    the quarter-round of a ``radius`` fillet from ``z0`` to ``z1``.
    """
    centre_x = x_face + sx * radius
    centre_y = y_face + sy * radius
    box = Part.makeBox(
        radius,
        radius,
        z1 - z0,
        App.Vector(min(x_face, centre_x), min(y_face, centre_y), z0),
    )
    cylinder = Part.makeCylinder(
        radius, z1 - z0, App.Vector(centre_x, centre_y, z0), App.Vector(0, 0, 1)
    )
    return box.cut(cylinder)


def _fuse(shape: Part.Shape, other: Part.Shape, what: str) -> Part.Shape:
    return require_one_solid(shape.fuse(other), what)


def _chamfered_plate(
    x0: float, y0: float, width: float, depth: float, z_floor: float, top: float
) -> Part.Shape:
    """One divider plate with the 0.4 mm top-edge chamfer."""
    plate = Part.makeBox(width, depth, top - z_floor, App.Vector(x0, y0, z_floor))
    top_edges = [
        edge
        for edge in shell.edges_at_z(plate, top)
        if edge.Length > _EDGE_LENGTH_TOLERANCE
    ]
    if not top_edges:
        raise GeometryError("divider plate: no top edges to chamfer")
    return plate.makeChamfer(DIVIDER_TOP_CHAMFER, top_edges)


def _plate(divider: Divider, z_floor: float, top: float) -> Part.Shape:
    """One tree divider plate, spanning its parent rectangle plus overlaps."""
    x0, y0, width, depth = divider.plate_bounds()
    return _chamfered_plate(x0, y0, width, depth, z_floor, top)


def build_dividers(
    body: Part.Shape, spec: shell.BodySpec, tree: dict
) -> Part.Shape:
    """Fuse the division tree's dividers into ``body``.

    Returns the body as one valid, closed solid with the plates, top
    chamfers, floor cove rails, and wall/T cove columns fused in. A leaf
    tree adds nothing, so the single-compartment path is unchanged.
    """
    if spec.units_x < 1 or spec.units_y < 1:
        raise GeometryError(
            f"footprint must be at least 1 x 1, got {spec.units_x} x {spec.units_y}"
        )
    z_floor = shell.FLOOR_TOP
    top = z_floor + divider_height(spec.height_u)
    shape = body

    for divider in divider_plan(spec, tree):
        shape = _fuse(shape, _plate(divider, z_floor, top), "divider plate")
        span_lo = divider.span_lo_mm - END_OVERLAP
        span_hi = divider.span_hi_mm + END_OVERLAP
        for face, side in divider.rail_faces():
            shape = _fuse(
                shape,
                cove_rail(divider.axis, face, side, span_lo, span_hi, z_floor),
                "cove rail",
            )
        for x_face, y_face, sx, sy in divider.junction_placements():
            # The plate's 0.4 mm top chamfer removes its top edge; end the
            # column at the chamfer's lower edge so the corner cannot stand
            # above the chamfered divider top.
            shape = _fuse(
                shape,
                cove_column(
                    x_face, y_face, sx, sy, z_floor, top - DIVIDER_TOP_CHAMFER
                ),
                "cove column",
            )

    return require_one_solid(shape, "body with dividers", closed=True)


def build_coupon_divider(body: Part.Shape, spec: shell.BodySpec) -> Part.Shape:
    """Fuse the calibration coupon's T-divider into ``body`` (ticket #10).

    Calibration section 2: one 2.0 mm T-divider fused to the floor and one
    wall, with its top 10 mm below the lip (28.0 mm on the 6U coupon). The
    plate is centred on the interior's x = 0 and runs
    :data:`COUPON_DIVIDER_LENGTH` from the -y wall's inner face into the
    interior, so the calibration's 20 mm weight foot rests on the 20 mm top.

    The wall-end floor and wall junctions take the same 2.0 mm cove rails and
    columns as a tree divider; the free end carries no column (there is no
    wall to fillet against) and the plate keeps its 0.4 mm top-edge chamfer.
    Like a tree divider, the wall columns stop at the chamfer's lower edge so
    no corner stands above the chamfered plate top. Only the 1 x 1 x 6U coupon
    exists, so any other geometry is a :class:`GeometryError`.
    """
    if (spec.units_x, spec.units_y, spec.height_u) != (1, 1, 6):
        raise GeometryError(
            "the coupon T-divider is fixed at 1 x 1 x 6U "
            f"(calibration section 2), got {spec.units_x} x {spec.units_y} x "
            f"{spec.height_u}U"
        )

    z_floor = shell.FLOOR_TOP
    top = z_floor + divider_height(spec.height_u)
    wall_face = -(gf.outer_width(spec.units_y) - 2.0 * shell.WALL) / 2.0
    free_end = wall_face + COUPON_DIVIDER_LENGTH
    near_face = -DIVIDER_THICKNESS / 2.0
    far_face = near_face + DIVIDER_THICKNESS

    shape = _fuse(
        body,
        _chamfered_plate(
            near_face,
            wall_face - END_OVERLAP,
            DIVIDER_THICKNESS,
            free_end - wall_face + END_OVERLAP,
            z_floor,
            top,
        ),
        "coupon T-divider plate",
    )
    for x_face, side in ((near_face, -1), (far_face, 1)):
        shape = _fuse(
            shape,
            cove_rail(
                "x", x_face, side, wall_face - END_OVERLAP, free_end, z_floor
            ),
            "coupon divider floor cove",
        )
        shape = _fuse(
            shape,
            cove_column(
                x_face,
                wall_face,
                side,
                1,
                z_floor,
                top - DIVIDER_TOP_CHAMFER,
            ),
            "coupon divider wall cove",
        )

    return require_one_solid(shape, "coupon body with T-divider", closed=True)
