# SPDX-License-Identifier: LGPL-2.1-or-later
"""Pinned Gridfinity base and stacking-lip profiles for boxgen.

This module is the single source of the Gridfinity interface in boxgen. It is
vendored, standalone, and importable under ``freecadcmd`` with no workbench
installed: it imports only ``FreeCAD`` and ``Part``.

Adapted from FreeCAD-Gridfinity-Workbench (LGPL-2.1-or-later):
    https://github.com/Stu142/FreeCAD-Gridfinity-Workbench
    Copyright (C) Stuart (Stu142) and contributors.

    - ``const.py``: the pinned profile dimensions.
    - ``utils.py``: ``create_rounded_rectangle`` (here
      :func:`rounded_rect_wire`), ``rounded_rectangle_chamfer`` and
      ``rounded_rectangle_extrude`` (here :func:`rounded_rect_loft` and
      :func:`rounded_rect_prism`).
    - ``feature_construction.py``: ``make_complex_bin_base`` (here
      :func:`make_base_foot`) and the profile-construction recipe (here
      :func:`lip_profile_points` and :func:`lip_inner_offset`).

The full license text is in ``cad/LICENSE-LGPL-2.1``. The base profile and
the rounded-rectangle construction come from the workbench. The lip follows
the official Gridfinity 4.4 mm profile (0.7 + 1.8 + 1.9, no flat top ledge);
the workbench's 4.0 mm variant is retained only as history in SYS-1.0.

Coordinate convention
=====================

* The **cell boundary** is x = 0 (and y = 0). A body's outer face sits
  ``CLEARANCE`` (0.25 mm) inside the cell boundary, so a 1-unit body is
  ``NOMINAL_WIDTH_1U`` (41.5 mm) wide and an N-unit body is
  :func:`outer_width` (N) wide. Footprint centres are one ``GRID`` apart.
* For the base, ``z = 0`` is the bed face and the profile occupies
  ``0 .. BASE_HEIGHT`` (4.75 mm). Each 42 mm cell gets its own foot; a
  multi-cell body has one foot per cell, and the feet are disjoint until they
  are fused into the body.
* For the lip, ``z = 0`` is the **lip bottom** -- the plane where the body
  wall ends and the lip begins -- and the profile reaches ``z = +LIP_HEIGHT``
  (4.4 mm) at the lip top. The skirt extends ``LIP_OVERLAP`` (2.4 mm) below
  the lip bottom to overlap and fuse with the body wall, so the solid profile
  spans ``-2.4 .. +4.4`` in z. In x, the workbench measures the profile inward
  from the cell boundary; :func:`lip_profile_points` keeps that convention,
  while :func:`lip_inner_offset` reports the same profile as an offset inward
  from the body outer face (what the shell, lid, and divider code need).
* :func:`make_lip_cut` returns the **void** the body cuts away: its wall is
  the material between the body outer face and the void. Cutting it leaves
  the official 4.4 mm lip with the 0.7 + 1.8 + 1.9 split and the 0.25 mm
  clearance to the next body's base. The top edge is the official sharp
  nominal edge; a print-only tip roundover is a separate, measured variant.

Construction note (verified under FreeCAD 1.0.0): the lip profile is not
monotonic -- its inner surface narrows, widens, then widens again as z rises
-- and a single multi-row loft through all rows has been observed to
self-intersect in OCCT 7.8 while still reporting ``isValid() == True`` and a
negative volume. The lip void is therefore built as five stacked, each
individually monotonic ruled lofts (plus the two straight extensions) and
fused -- the piecewise recipe the FreeCAD API notes recommend. Every loft and
boolean here is checked for a positive volume and exactly one solid before it
is returned.
"""

from __future__ import annotations

import math
from collections.abc import Sequence

import FreeCAD as fc
import Part

__all__ = [
    "BASE_BOTTOM_CHAMFER",
    "BASE_BOTTOM_RADIUS",
    "BASE_HEIGHT",
    "BASE_TOP_CHAMFER",
    "BASE_VERTICAL",
    "BASE_VERTICAL_RADIUS",
    "CLEARANCE",
    "GRID",
    "GridfinityError",
    "LIP_BOTTOM_CHAMFER",
    "LIP_HEIGHT",
    "LIP_OVERLAP",
    "LIP_TOP_CHAMFER",
    "LIP_TOP_LEDGE",
    "LIP_VERTICAL",
    "LIP_WALL",
    "NOMINAL_WIDTH_1U",
    "OUTER_RADIUS",
    "lip_inner_offset",
    "lip_profile_points",
    "make_base_feet",
    "make_base_foot",
    "make_lip_cut",
    "make_stacking_lip",
    "outer_width",
    "rounded_rect_loft",
    "rounded_rect_prism",
    "rounded_rect_wire",
]

# --- Gridfinity interface (design standard SYS-1.0 section 3, ADR-0010) ------

#: Grid pitch in millimetres.
GRID = 42.0

#: Clearance per side between a body and the cell boundary (and between
#: stacked bodies).
CLEARANCE = 0.25

#: Nominal outer width of a 1-unit body: 42 - 2 * 0.25.
NOMINAL_WIDTH_1U = GRID - 2.0 * CLEARANCE

#: Base profile components, bottom to top (0.8 + 1.8 + 2.15 mm).
BASE_BOTTOM_CHAMFER = 0.8
BASE_VERTICAL = 1.8
BASE_TOP_CHAMFER = 2.15

#: Base profile height, 4.75 mm, as the sum of its components.
BASE_HEIGHT = BASE_BOTTOM_CHAMFER + BASE_VERTICAL + BASE_TOP_CHAMFER

#: Corner radii of the base profile at its three shelf levels.
BASE_BOTTOM_RADIUS = 0.8
BASE_VERTICAL_RADIUS = 1.6

#: Outer vertical corner radius of every body, standard section 4.1.
OUTER_RADIUS = 3.75

#: Stacking lip components: the official Gridfinity split is
#: 0.7 + 1.8 + 1.9 mm, with no flat top ledge. The top chamfer is derived
#: from the base profile minus the 0.25 mm per-side clearance.
LIP_TOP_LEDGE = 0.0
LIP_BOTTOM_CHAMFER = 0.7
LIP_VERTICAL = 1.8
LIP_TOP_CHAMFER = BASE_TOP_CHAMFER - CLEARANCE - LIP_TOP_LEDGE

#: Official stacking lip height above the body wall, 4.4 mm, as the sum of
#: its components.
LIP_HEIGHT = LIP_BOTTOM_CHAMFER + LIP_VERTICAL + LIP_TOP_CHAMFER

#: Nominal wall thickness at the lip (the workbench's ``WallThickness``).
LIP_WALL = 2.0

#: How far the lip skirt reaches below the lip bottom to fuse with the body
#: wall: ``-z5`` of the workbench profile, 2.4 mm.
LIP_OVERLAP = (
    LIP_VERTICAL + LIP_TOP_LEDGE + LIP_TOP_CHAMFER + LIP_BOTTOM_CHAMFER - LIP_WALL
)


class GridfinityError(RuntimeError):
    """A pinned-profile request that cannot produce valid geometry."""


def outer_width(units: int) -> float:
    """Return the nominal outer width of a body spanning ``units`` grid cells.

    One unit is ``NOMINAL_WIDTH_1U`` (41.5 mm): the 42 mm pitch minus the
    0.25 mm clearance per side. ``units * GRID - 2 * CLEARANCE``.
    """
    return _units(units, "units") * GRID - 2.0 * CLEARANCE


# --- Rounded-rectangle solids (adapted from utils.py) ------------------------


def rounded_rect_wire(
    width: float,
    depth: float,
    radius: float,
    z: float = 0.0,
    cx: float = 0.0,
    cy: float = 0.0,
) -> Part.Wire:
    """Return a closed rounded rectangle in the XY plane.

    The wire is centred at ``(cx, cy, z)``. ``radius`` must be positive and
    smaller than half of both ``width`` and ``depth``. This is the workbench's
    ``create_rounded_rectangle`` with an explicit centre.
    """
    if width <= 0.0 or depth <= 0.0:
        raise GridfinityError(f"rounded rectangle needs positive sizes, got {width} x {depth}")
    if radius <= 0.0:
        raise GridfinityError(f"rounded rectangle radius must be positive, got {radius}")
    if radius >= width / 2.0 or radius >= depth / 2.0:
        raise GridfinityError(
            f"radius {radius} must be smaller than half of {width} x {depth}",
        )

    x = width / 2.0
    y = depth / 2.0
    xc = x - radius
    yc = y - radius
    xa = xc + radius * math.sin(math.pi / 4.0)
    ya = yc + radius * math.sin(math.pi / 4.0)

    def point(px: float, py: float) -> fc.Vector:
        return fc.Vector(cx + px, cy + py, z)

    curves = [
        Part.LineSegment(point(-xc, y), point(xc, y)),
        Part.Arc(point(xc, y), point(xa, ya), point(x, yc)),
        Part.LineSegment(point(x, yc), point(x, -yc)),
        Part.Arc(point(x, -yc), point(xa, -ya), point(xc, -y)),
        Part.LineSegment(point(xc, -y), point(-xc, -y)),
        Part.Arc(point(-xc, -y), point(-xa, -ya), point(-x, -yc)),
        Part.LineSegment(point(-x, -yc), point(-x, yc)),
        Part.Arc(point(-x, yc), point(-xa, ya), point(-xc, y)),
    ]
    return Part.Wire([curve.toShape() for curve in curves])


def rounded_rect_prism(
    width: float,
    depth: float,
    radius: float,
    z0: float,
    z1: float,
    cx: float = 0.0,
    cy: float = 0.0,
) -> Part.Shape:
    """Extrude a rounded rectangle from ``z0`` to ``z1`` (a straight prism)."""
    if z1 <= z0:
        raise GridfinityError(f"prism needs z1 > z0, got {z0} .. {z1}")
    face = Part.Face(rounded_rect_wire(width, depth, radius, z0, cx, cy))
    return face.extrude(fc.Vector(0.0, 0.0, z1 - z0))


def rounded_rect_loft(
    z0: float,
    width0: float,
    depth0: float,
    radius0: float,
    z1: float,
    width1: float,
    depth1: float,
    radius1: float,
    cx: float = 0.0,
    cy: float = 0.0,
) -> Part.Shape:
    """Ruled loft between two rounded rectangles, one straight profile segment.

    ``z1`` must be greater than ``z0``: each segment is monotonic by itself,
    which is what keeps the lip's non-monotonic overall profile safe.
    """
    if z1 <= z0:
        raise GridfinityError(f"loft needs z1 > z0, got {z0} .. {z1}")
    bottom = rounded_rect_wire(width0, depth0, radius0, z0, cx, cy)
    top = rounded_rect_wire(width1, depth1, radius1, z1, cx, cy)
    return Part.makeLoft([bottom, top], solid=True, ruled=True)


def _single_solid(shape: Part.Shape, what: str) -> Part.Shape:
    """Return ``shape`` as one positive solid or raise.

    Kept local on purpose: this vendored module must stay importable on its
    own (only ``FreeCAD`` and ``Part``; see the module docstring), so it
    cannot use ``cad/geometry.py``'s shared ``require_one_solid`` the way the
    boxgen modules do. Boolean results come back as a compound wrapping one
    solid; a loft can silently self-intersect with a negative volume, so both
    invariants are checked at every construction step.
    """
    if shape.isNull():
        raise GridfinityError(f"{what}: empty shape")
    if shape.Volume <= 0.0:
        raise GridfinityError(f"{what}: non-positive volume {shape.Volume}")
    solids = shape.Solids
    if len(solids) != 1:
        raise GridfinityError(f"{what}: expected one solid, got {len(solids)}")
    return solids[0] if shape.ShapeType == "Compound" else shape


def _units(value: object, what: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 1:
        raise GridfinityError(
            f"{what} must be a positive whole number of grid units, got {value!r}",
        )
    return value


def _footprint(footprint: Sequence[int]) -> tuple[int, int]:
    try:
        units_x, units_y = footprint
    except (TypeError, ValueError) as error:
        raise GridfinityError(
            f"footprint must be a (units_x, units_y) pair, got {footprint!r}",
        ) from error
    return _units(units_x, "footprint x"), _units(units_y, "footprint y")


# --- Base profile (adapted from make_complex_bin_base) -----------------------


def make_base_foot(
    cell_origin_x: float = 0.0,
    cell_origin_y: float = 0.0,
    lift: float = 0.0,
) -> Part.Shape:
    """Build the 4.75 mm Gridfinity base foot for one 42 mm cell.

    The foot is centred in its cell, at ``cell_origin + GRID / 2`` on both
    axes, so a body covering cells ``(i, j)`` places feet at
    ``(i * GRID, j * GRID)``. Three monotonic ruled lofts form the profile:

    ==========  =================  ==============
    z (mm)      1-unit width (mm)  corner radius
    ==========  =================  ==============
    0.0         35.6               0.8
    0.8         37.2               1.6
    2.6         37.2               1.6
    4.75        41.5               3.75
    ==========  =================  ==============

    ``lift`` extends the foot straight up past ``BASE_HEIGHT`` by that many
    millimetres at the full 41.5 mm width. The shell builder uses a small
    lift (0.2 mm) so the foot overlaps the body instead of sharing a face;
    the pinned profile itself is unchanged.
    """
    if lift < 0.0:
        raise GridfinityError(f"lift must be >= 0, got {lift}")

    center_x = cell_origin_x + GRID / 2.0
    center_y = cell_origin_y + GRID / 2.0
    bottom_width = NOMINAL_WIDTH_1U - 2.0 * BASE_BOTTOM_CHAMFER - 2.0 * BASE_TOP_CHAMFER
    vertical_width = NOMINAL_WIDTH_1U - 2.0 * BASE_TOP_CHAMFER
    vertical_top = BASE_BOTTOM_CHAMFER + BASE_VERTICAL

    foot = rounded_rect_loft(
        0.0,
        bottom_width,
        bottom_width,
        BASE_BOTTOM_RADIUS,
        BASE_BOTTOM_CHAMFER,
        vertical_width,
        vertical_width,
        BASE_VERTICAL_RADIUS,
        center_x,
        center_y,
    )
    foot = foot.fuse(
        rounded_rect_prism(
            vertical_width,
            vertical_width,
            BASE_VERTICAL_RADIUS,
            BASE_BOTTOM_CHAMFER,
            vertical_top,
            center_x,
            center_y,
        ),
    )
    foot = foot.fuse(
        rounded_rect_loft(
            vertical_top,
            vertical_width,
            vertical_width,
            BASE_VERTICAL_RADIUS,
            BASE_HEIGHT,
            NOMINAL_WIDTH_1U,
            NOMINAL_WIDTH_1U,
            OUTER_RADIUS,
            center_x,
            center_y,
        ),
    )
    if lift > 0.0:
        foot = foot.fuse(
            rounded_rect_prism(
                NOMINAL_WIDTH_1U,
                NOMINAL_WIDTH_1U,
                OUTER_RADIUS,
                BASE_HEIGHT,
                BASE_HEIGHT + lift,
                center_x,
                center_y,
            ),
        )
    return _single_solid(foot, "base foot")


def make_base_feet(footprint: Sequence[int], lift: float = 0.0) -> Part.Shape:
    """Build one base foot per cell of ``(units_x, units_y)`` and fuse them.

    The feet sit on a 42 mm pitch from the origin, so the whole base spans
    ``CLEARANCE .. units * GRID - CLEARANCE`` and is ``outer_width(units)``
    wide. Neighbouring feet are disjoint -- 0.5 mm of clearance separates
    them, exactly as the standard separates modules -- so the result is a
    compound with one solid per cell until a body fuses it into one piece.
    """
    units_x, units_y = _footprint(footprint)
    feet = [
        make_base_foot(cell_x * GRID, cell_y * GRID, lift)
        for cell_x in range(units_x)
        for cell_y in range(units_y)
    ]
    if len(feet) == 1:
        return feet[0]
    merged = feet[0].multiFuse(feet[1:])
    if merged.Volume <= 0.0:
        raise GridfinityError(f"base feet: non-positive volume {merged.Volume}")
    return merged


# --- Stacking lip (adapted from _stacking_lip_profile) -----------------------


def lip_profile_points() -> tuple[tuple[float, float], ...]:
    """Return the closed lip cross-section in workbench coordinates.

    Points are ``(x, z)`` in millimetres in the profile plane: x is measured
    inward from the **cell boundary** (the body outer face is at
    ``x = CLEARANCE``), and z is measured from the **lip bottom** (positive
    up to the lip top, negative down into the body). The eight points are the
    official Gridfinity stacking-lip polygon:

    ``(0.25, 0) (0.25, 4.4) (2.15, 2.5) (2.15, 0.7) (2.85, 0)
    (2.85, -1.8) (2.25, -2.4) (0.25, -2.4)``.
    """
    x1 = CLEARANCE
    x3 = x1 + LIP_TOP_CHAMFER
    x4 = x3 + LIP_BOTTOM_CHAMFER
    x5 = CLEARANCE + LIP_WALL
    return (
        (x1, 0.0),
        (x1, LIP_HEIGHT),
        (x3, LIP_BOTTOM_CHAMFER + LIP_VERTICAL),
        (x3, LIP_BOTTOM_CHAMFER),
        (x4, 0.0),
        (x4, -LIP_VERTICAL),
        (x5, -LIP_OVERLAP),
        (x1, -LIP_OVERLAP),
    )


#: The six profile rows as ``(z relative to the lip bottom, offset inward
#: from the body outer face)``. Straight lines connect consecutive rows;
#: outside the range the nearest row's offset is held.
_LIP_OFFSET_ROWS = (
    (-LIP_OVERLAP, LIP_WALL),
    (-LIP_VERTICAL, LIP_TOP_LEDGE + LIP_TOP_CHAMFER + LIP_BOTTOM_CHAMFER),
    (0.0, LIP_TOP_LEDGE + LIP_TOP_CHAMFER + LIP_BOTTOM_CHAMFER),
    (LIP_BOTTOM_CHAMFER, LIP_TOP_LEDGE + LIP_TOP_CHAMFER),
    (LIP_BOTTOM_CHAMFER + LIP_VERTICAL, LIP_TOP_LEDGE + LIP_TOP_CHAMFER),
    (LIP_HEIGHT, LIP_TOP_LEDGE),
)


def lip_inner_offset(z: float) -> float:
    """Return the lip's inner-surface offset at ``z`` (relative to the lip bottom).

    The offset is measured inward from the body outer face, so an inner width
    at that height is ``outer_width(units) - 2 * offset`` and an inner corner
    radius is ``OUTER_RADIUS - offset``. Below the lip this is the body wall
    (2.0 mm), then the profile narrows to 2.6 mm at the neck, steps back to
    1.9 mm for the vertical section, and reaches 0.4 mm at the top ledge.
    """
    rows = _LIP_OFFSET_ROWS
    if z <= rows[0][0]:
        return rows[0][1]
    if z >= rows[-1][0]:
        return rows[-1][1]
    for (z0, offset0), (z1, offset1) in zip(rows, rows[1:]):
        if z0 <= z <= z1:
            if z1 == z0:
                return offset1
            fraction = (z - z0) / (z1 - z0)
            return offset0 + fraction * (offset1 - offset0)
    raise GridfinityError(f"lip inner offset is undefined at z={z}")


def make_lip_cut(outer_wire: Part.Wire, margin: float = 0.5) -> Part.Shape:
    """Return the lip void for the body contour ``outer_wire`` (a solid to cut).

    ``outer_wire`` is the body's outer contour -- the wire from
    :func:`rounded_rect_wire` at ``outer_width(units)`` with ``OUTER_RADIUS``
    corners, in the plane of the lip bottom. The returned solid spans
    ``z = -LIP_OVERLAP - margin`` to ``z = LIP_HEIGHT + margin`` around that
    plane and is built from five stacked monotonic lofts plus straight
    extensions, so it is safe under OCCT 7.8. Its cross-section at a height
    is the lip's inner opening there: 37.5 mm at the bottom (the body wall
    line), 36.3 mm at the neck, 37.7 mm through the vertical section, and
    41.5 mm at the top for a 1-unit body.
    """
    if not isinstance(outer_wire, Part.Wire):
        raise GridfinityError(f"outer_wire must be a Part.Wire, got {type(outer_wire).__name__}")
    if margin <= 0.0:
        raise GridfinityError(f"margin must be positive, got {margin}")
    box = outer_wire.BoundBox
    width, depth = box.XLength, box.YLength
    center_x, center_y, lip_bottom = box.Center.x, box.Center.y, box.Center.z

    pieces = [
        rounded_rect_prism(
            width - 2.0 * _LIP_OFFSET_ROWS[0][1],
            depth - 2.0 * _LIP_OFFSET_ROWS[0][1],
            OUTER_RADIUS - _LIP_OFFSET_ROWS[0][1],
            lip_bottom - LIP_OVERLAP - margin,
            lip_bottom - LIP_OVERLAP,
            center_x,
            center_y,
        ),
    ]
    for (z0, offset0), (z1, offset1) in zip(_LIP_OFFSET_ROWS, _LIP_OFFSET_ROWS[1:]):
        pieces.append(
            rounded_rect_loft(
                lip_bottom + z0,
                width - 2.0 * offset0,
                depth - 2.0 * offset0,
                OUTER_RADIUS - offset0,
                lip_bottom + z1,
                width - 2.0 * offset1,
                depth - 2.0 * offset1,
                OUTER_RADIUS - offset1,
                center_x,
                center_y,
            ),
        )
    pieces.append(
        rounded_rect_prism(
            width - 2.0 * _LIP_OFFSET_ROWS[-1][1],
            depth - 2.0 * _LIP_OFFSET_ROWS[-1][1],
            OUTER_RADIUS - _LIP_OFFSET_ROWS[-1][1],
            lip_bottom + LIP_HEIGHT,
            lip_bottom + LIP_HEIGHT + margin,
            center_x,
            center_y,
        ),
    )

    cut = pieces[0]
    for piece in pieces[1:]:
        cut = cut.fuse(piece)
    return _single_solid(cut, "lip cut")


def make_stacking_lip(outer_wire: Part.Wire) -> Part.Shape:
    """Return the solid stacking-lip ring for the body contour ``outer_wire``.

    The ring is the material between the body's outer contour and the lip
    void from :func:`make_lip_cut`; its outer face is flush with the body
    outer face, so a 1-unit ring is 41.5 x 41.5 mm and spans
    ``-2.4 .. +4.4`` mm around the lip bottom plane. Fuse it onto the top of
    a hollow body wall, or cut :func:`make_lip_cut` directly if the shell
    builder prefers to cut the void (both leave the same interface).
    """
    if not isinstance(outer_wire, Part.Wire):
        raise GridfinityError(f"outer_wire must be a Part.Wire, got {type(outer_wire).__name__}")
    ring = Part.Face(outer_wire).extrude(fc.Vector(0.0, 0.0, LIP_HEIGHT + LIP_OVERLAP))
    ring = ring.translated(fc.Vector(0.0, 0.0, -LIP_OVERLAP))
    ring = ring.cut(make_lip_cut(outer_wire))
    return _single_solid(ring, "stacking lip")
