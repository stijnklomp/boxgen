"""SYS-1.0 body shell: solid base zone, body wall, and stacking lip.

Ticket #6 builds the body shell; the division tree's dividers (ticket #8)
fuse into the shell through :mod:`cad.dividers`, and the lid (ticket #9) is
built beside it.

The recipe is the verified FreeCAD 1.0.0 one: a rounded-rectangle prism for
the body wall, one pinned Gridfinity foot per 42 mm cell (lifted 0.2 mm into
the body so the fuse overlaps instead of sharing a face), a pre-filleted
interior cutter for the 2.0 mm floor-to-wall cove, cell webs that cap the
0.5 mm Gridfinity grooves over the lift band so interior feet stay fused, and
the pinned stacking-lip void.

The base zone is solid: the floor is the top face of the base profile and the
underside is the full foot face, so the bottom prints on the bed instead of
bridging a hidden cavity. The hidden 1.2 mm-side-shell cavity of ADR-0007 was
removed by ADR-0013 after the coupon showed that its 35 mm unsupported
ceiling cannot be printed in PETG.

Every fuse and cut is checked for exactly one solid with positive volume.
Internal fillets are made by filleting the cutting solid, never the fused
shell: OCC cannot fillet the shell's floor edges directly. The fillet with
its 2.0 -> 1.0 mm reduce-then-fail contract lives in :mod:`cad.fillets`;
build_body reports the radius actually applied.

:func:`measure_interior_depth_mm` measures the usable depth from the built
body (floor top to lip bottom) for the report; :func:`interior_depth_mm`
stays the standard's design formula.
"""

from __future__ import annotations

from dataclasses import dataclass

import FreeCAD as App
import Part

from . import gridfinity as gf
from .fillets import FILLET_FALLBACK, fillet_with_fallback
from .geometry import GeometryError, require_one_solid

__all__ = [
    "BASE_LIFT",
    "BodyResult",
    "BodySpec",
    "FILLET_FALLBACK",
    "FLOOR_FILLET",
    "GeometryError",
    "HEIGHT_UNIT",
    "INNER_RADIUS",
    "LIP_CUTTER_GAP",
    "WALL",
    "build_body",
    "edges_at_z",
    "interior_depth_mm",
    "make_cell_webs",
    "measure_interior_depth_mm",
]

#: Height of one Gridfinity unit in millimetres.
HEIGHT_UNIT = 7.0

#: Body wall thickness (standard §4). The base zone below the floor is solid;
#: its top face is the floor (see :func:`measure_interior_depth_mm`).
WALL = 2.0

#: Half-width of the z band the floor-top bisection searches around the base
#: top: the floor top is the top of the solid base zone, so the band only
#: needs to straddle ``gf.BASE_HEIGHT``.
_FLOOR_PROBE_BAND = 1.0

#: Inner vertical corner radius: outer radius minus the 2.0 mm wall.
INNER_RADIUS = gf.OUTER_RADIUS - WALL

#: Floor-to-wall fillet, with the standard's 1.0 mm fallback.
FLOOR_FILLET = 2.0

#: How far each base foot is lifted into the body so the fuse overlaps.
BASE_LIFT = 0.2

#: The Gridfinity groove between adjacent base feet (one clearance per side).
CELL_GROOVE = gf.GRID - gf.NOMINAL_WIDTH_1U

#: Width of the cell-boundary web: the groove plus an overlap into the two
#: neighbouring feet, so the fuse never relies on coincident faces.
WEB_WIDTH = CELL_GROOVE + 2.0 * BASE_LIFT

#: The body interior cutter stops this far below the lip bottom. The lip
#: profile starts narrowing 2.4 mm below the lip bottom; 2.3 mm leaves the
#: pinned neck intact (verified against the workbench profile).
LIP_CUTTER_GAP = 2.3


@dataclass(frozen=True)
class BodySpec:
    """The three variable body dimensions the master modules need.

    ``units_x`` / ``units_y`` are the Gridfinity footprint in cells and
    ``height_u`` the height in units. The full spec dict carries more (id,
    the division tree, the lid flags, contents), but only these three travel
    through the geometry modules; this is the one object every master
    function takes instead of a ``(units_x, units_y, height_u)`` tuple.
    """

    units_x: int
    units_y: int
    height_u: int


@dataclass(frozen=True)
class BodyResult:
    """A built body and the floor fillet radius actually applied.

    ``floor_fillet_mm`` is the 2.0 mm standard radius, or the 1.0 mm fallback
    when 2.0 could not be applied; the build report records it and warns on
    the fallback. A sharp internal corner is never returned: the helper
    raises :class:`GeometryError` instead.
    """

    shape: Part.Shape
    floor_fillet_mm: float


def edges_at_z(shape: Part.Shape, z: float, tolerance: float = 1e-6) -> list[Part.Edge]:
    """Return the edges whose whole bounding box lies at height ``z``.

    Vertex order is not stable across booleans and degenerate edges can lack
    vertices or a curve, so filter with each edge's ``BoundBox``.
    """
    return [
        edge
        for edge in shape.Edges
        if abs(edge.BoundBox.ZMin - z) < tolerance
        and abs(edge.BoundBox.ZMax - z) < tolerance
    ]


def interior_depth_mm(height_u: int) -> float:
    """Design interior depth: ``U * 7 - 4.75 - 4.0`` (standard §4.2 table).

    This is the standard's design figure (``U * 7 - 8.75``); the build report
    carries the value measured from the built body instead, see
    :func:`measure_interior_depth_mm`.
    """
    return height_u * HEIGHT_UNIT - gf.BASE_HEIGHT - gf.LIP_HEIGHT


#: Bisection steps for the measured interior-depth boundaries.
_DEPTH_BISECTIONS = 24

#: Width tolerance of the lip's neck plateau in the lip-bottom measurement.
_NECK_TOLERANCE = 1e-4


def _section_openings(shape: Part.Shape, z: float) -> list[float]:
    """Widths of the inner wires (the openings) of the section at ``z``.

    A wire is an opening when another wire's bounding box contains it: a
    solid section of separate base feet has no such wire, the interior has one
    per compartment, and the lip region has the neck opening.
    """
    wires = list(shape.slice(App.Vector(0.0, 0.0, 1.0), z))
    openings = []
    for wire in wires:
        box = wire.BoundBox
        contained = any(
            other is not wire
            and other.BoundBox.XMin <= box.XMin + 1e-6
            and other.BoundBox.XMax >= box.XMax - 1e-6
            and other.BoundBox.YMin <= box.YMin + 1e-6
            and other.BoundBox.YMax >= box.YMax - 1e-6
            for other in wires
        )
        if contained:
            openings.append(box.XLength)
    return openings


def measure_interior_depth_mm(body: Part.Shape, height_u: int) -> float:
    """Measure the usable interior depth from the built body (standard §4.2).

    ``interior_depth_mm(height_u)`` stays the design figure the standard's
    table states; the build report carries this measurement of the model:

    * the **floor top** is the lowest z whose horizontal section is hollow:
      the interior opening appears above the solid base zone, so the
      bisection runs around the base top and finds the floor's top face;
    * the **lip bottom** is the highest z at which the lip's inner opening is
      still at the neck's minimum width: the neck plateau ends exactly at the
      lip bottom, where the pinned profile starts widening toward the lip's
      vertical section.

    Both boundaries are bracketed with the design constants and then found on
    the built shape, so a geometry change moves the reported depth; the design
    formula is never reported silently.
    """
    total_h = height_u * HEIGHT_UNIT
    z_lip = total_h - gf.LIP_HEIGHT

    low = gf.BASE_HEIGHT - _FLOOR_PROBE_BAND
    high = gf.BASE_HEIGHT + _FLOOR_PROBE_BAND
    for _ in range(_DEPTH_BISECTIONS):
        middle = (low + high) / 2.0
        if _section_openings(body, middle):
            high = middle
        else:
            low = middle
    floor_top = (low + high) / 2.0

    probe = z_lip - 1.0
    openings = _section_openings(body, probe)
    if not openings:
        raise GeometryError(f"interior depth: no lip opening at z={probe}")
    neck_width = max(openings)
    low = probe
    high = probe + 2.0
    for _ in range(_DEPTH_BISECTIONS):
        middle = (low + high) / 2.0
        openings = _section_openings(body, middle)
        if openings and max(openings) <= neck_width + _NECK_TOLERANCE:
            low = middle
        else:
            high = middle
    lip_bottom = (low + high) / 2.0

    depth = lip_bottom - floor_top
    if depth <= 0.0:
        raise GeometryError(
            f"interior depth: measured {depth:.3f} mm between the floor top "
            f"({floor_top:.3f} mm) and the lip bottom ({lip_bottom:.3f} mm)"
        )
    return depth


def make_cell_webs(
    spec: BodySpec, z0: float, height: float
) -> list[Part.Shape]:
    """One web per internal 42 mm cell boundary, capping the base grooves.

    The interior cutter removes the body prism above the base zone, so an
    interior foot stands below the void with nothing but its own edges to
    hold it: the 0.5 mm Gridfinity groove between adjacent feet separates
    them, so without a web the interior feet of a 4 x 4 body would be
    separate solids (the shell would come out as five). A web fills each
    internal groove over a band above the base top and overlaps both
    neighbouring feet by :data:`BASE_LIFT`, tying every interior foot to the
    body wall.

    The web cannot affect the Gridfinity fit: it sits at ``z0 >=
    BASE_HEIGHT`` (the top of the base zone), where a standard baseplate's
    cell rib has narrowed to zero width, and its ends run 1.0 mm into the
    walls (the span is the outer width minus ``2 * (WALL - 1.0)``), where
    the body prism already fills the band.
    """
    span_x = gf.outer_width(spec.units_x) - 2.0 * (WALL - 1.0)
    span_y = gf.outer_width(spec.units_y) - 2.0 * (WALL - 1.0)

    webs = []
    for index_x in range(1, spec.units_x):
        centre = (index_x - spec.units_x / 2.0) * gf.GRID
        webs.append(
            Part.makeBox(
                WEB_WIDTH,
                span_y,
                height,
                App.Vector(centre - WEB_WIDTH / 2.0, -span_y / 2.0, z0),
            )
        )
    for index_y in range(1, spec.units_y):
        centre = (index_y - spec.units_y / 2.0) * gf.GRID
        webs.append(
            Part.makeBox(
                span_x,
                WEB_WIDTH,
                height,
                App.Vector(-span_x / 2.0, centre - WEB_WIDTH / 2.0, z0),
            )
        )
    return webs


def _fuse(shape: Part.Shape, other: Part.Shape, what: str) -> Part.Shape:
    return require_one_solid(shape.fuse(other), what)


def _cut(shape: Part.Shape, other: Part.Shape, what: str) -> Part.Shape:
    return require_one_solid(shape.cut(other), what)


def build_body(spec: BodySpec) -> BodyResult:
    """Build one body: solid base zone, 2.0 mm wall, and stacking lip.

    The base zone is solid (ADR-0013): one pinned Gridfinity foot per cell,
    fused to the wall prism, with no underside cavity, so the bottom layers
    print on the bed instead of bridging a hidden void. The interior floor is
    the top of that base zone (``gf.BASE_HEIGHT``), and the interior floor
    walls meet it in the standard's 2.0 mm cove.

    The body is centred in X/Y, ``z = 0`` is the bed face, and all dimensions
    are millimetres. The body contains the bare shell only; the division
    tree's dividers fuse in through :mod:`cad.dividers`. Returns the shape
    and the floor fillet radius actually applied (2.0, or the 1.0 fallback
    when reduce was required).
    """
    if spec.units_x < 1 or spec.units_y < 1:
        raise GeometryError(
            f"footprint must be at least 1 x 1, got {spec.units_x} x {spec.units_y}"
        )
    if spec.height_u < 3:
        raise GeometryError(f"height must be at least 3U, got {spec.height_u}U")

    total_h = spec.height_u * HEIGHT_UNIT
    z_floor = gf.BASE_HEIGHT
    z_lip = total_h - gf.LIP_HEIGHT
    if z_lip - LIP_CUTTER_GAP <= z_floor:
        raise GeometryError(
            f"{spec.height_u}U leaves no body interior between the floor and the lip"
        )

    width = gf.outer_width(spec.units_x)
    depth = gf.outer_width(spec.units_y)

    body = require_one_solid(
        gf.rounded_rect_prism(width, depth, gf.OUTER_RADIUS, z_floor, total_h),
        "outer body prism",
    )

    for index_x in range(spec.units_x):
        for index_y in range(spec.units_y):
            origin_x = (
                index_x - (spec.units_x - 1) / 2.0
            ) * gf.GRID - gf.GRID / 2.0
            origin_y = (
                index_y - (spec.units_y - 1) / 2.0
            ) * gf.GRID - gf.GRID / 2.0
            body = _fuse(
                body,
                gf.make_base_foot(origin_x, origin_y, lift=BASE_LIFT),
                f"base foot {index_x},{index_y}",
            )

    inner = gf.rounded_rect_prism(
        width - 2.0 * WALL,
        depth - 2.0 * WALL,
        INNER_RADIUS,
        z_floor,
        z_lip - LIP_CUTTER_GAP,
    )
    inner, floor_fillet_mm = fillet_with_fallback(
        inner,
        FLOOR_FILLET,
        edges_at_z(inner, z_floor),
        fallback=FILLET_FALLBACK,
        what="body interior floor fillet",
    )
    body = body.cut(inner)

    # The interior cut separates base feet that do not reach a wall (a 4 x 4
    # body's four centre feet); the cell webs tie every foot back to the
    # prism, so the one-solid invariant is checked after the webs.
    for web in make_cell_webs(spec, z_floor, BASE_LIFT):
        body = body.fuse(web)
    body = require_one_solid(body, "body interior with cell webs")

    lip_wire = gf.rounded_rect_wire(width, depth, gf.OUTER_RADIUS, z_lip)
    body = _cut(body, gf.make_lip_cut(lip_wire), "stacking lip void")

    return BodyResult(
        shape=require_one_solid(body, "body"), floor_fillet_mm=floor_fillet_mm
    )
