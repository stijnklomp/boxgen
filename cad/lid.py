"""Inset friction lid (SYS-1.0 section 4.4, ADR-0005).

The lid is a removable plate that sits inside the lip zone; it never carries
stack load and adds no pitch. Verified geometry (FreeCAD API notes section 7,
ported from the prototype):

* **Plate** 2.0 mm thick. Its top sits 3.0 mm above the lip bottom, which is
  1.4 mm below the official 4.4 mm lip top
  (``z_top = z_lip + SKIRT_INTRUSION + LID_PLATE``). The lid geometry
  relative to the lip is unchanged from SYS-1.0; only the lip top moved up by
  0.4 mm. Lids are top-only and never sit under another body.
* **Skirt** a 2.0 mm thick ring that follows the lip's inner profile with
  0.30 mm clearance per side and reaches 1.0 mm below the lip bottom. The
  plate is ``outer_width(units) - 2 * (LIP_VERTICAL_OFFSET + LID_CLEARANCE)``
  wide, where ``LIP_VERTICAL_OFFSET`` is derived from the pinned profile's
  ledge and top chamfer, so it clears the lip's vertical section by
  ``LID_CLEARANCE`` per side and stops on the lip neck's shoulder. The skirt
  outer contour is the lip profile plus the clearance, built as stacked
  monotonic segments: one multi-row loft through the non-monotonic lip
  profile self-intersects under OCCT 7.8.
* **Edges**: 0.6 mm chamfer on the skirt bottom (self-alignment), 1.0 mm
  roundover on the plate top (handling). The roundover is optional-by-design:
  a 1.0 mm roundover on a 2.0 mm plate is at the geometric limit, so a
  rejected fillet leaves the sharp-but-valid plate instead of failing the
  build. ``top_roundover_applied`` reports what was applied so the build
  report can record it and warn.
* **Finger hole** 20 mm diameter through the plate, centred 14 mm from the
  **+y** edge of the body's outer face (``outer_width(units_y) / 2 - 14``).
  The standard says "14 mm from one edge"; the reference edge is the body
  footprint edge, which is the prototype's verified formula. +y is chosen
  because the body is generated as given (``footprint: [N, M]``, larger side
  not required first), so +y is the deterministic "top" edge in plan view
  for every orientation.
* **Ribs** (``ribs=True``): the standard's optional pair of 0.15 mm friction
  ribs, two boxes 10 mm long x 1.5 mm tall on the +x and -x skirt faces,
  0.15 mm proud of the pilot band. The pilot faces the lip's neck, where the
  clearance is 0.30 mm, so the ribs halve it locally; a straight box cannot
  follow the widening seating frustum above the pilot without colliding with
  the lip, so the visible rib is the pilot band. They stay inside the plate
  footprint, so nothing protrudes beyond the print (standard section 4.5).
  The coupon (ticket #10) tunes the physical fit; the generator only offers
  the switch.

Every construction step asserts one valid solid with positive volume and one
closed shell; a failure raises :class:`~cad.fillets.GeometryError`, which the
build maps to exit code 3.
"""

from __future__ import annotations

import Part
import FreeCAD as App

from . import gridfinity as gf
from .geometry import GeometryError, require_one_solid
from .shell import HEIGHT_UNIT, BodySpec, edges_at_z

__all__ = [
    "FINGER_HOLE_DIAMETER",
    "FINGER_HOLE_EDGE_DISTANCE",
    "LID_CLEARANCE",
    "LID_CORNER_RADIUS",
    "LID_PLATE",
    "LID_RIB_HEIGHT",
    "LID_RIB_LENGTH",
    "LID_RIB_PROUD",
    "LID_SKIRT",
    "LID_SKIRT_CHAMFER",
    "LID_TOP_ROUNDOVER",
    "SKIRT_INTRUSION",
    "build_lid",
    "top_roundover_applied",
]

#: Lid plate thickness (standard section 4.4).
LID_PLATE = 2.0

#: Lid skirt thickness (standard section 4.4).
LID_SKIRT = 2.0

#: Skirt clearance per side to the lip (standard section 4.4/6).
LID_CLEARANCE = 0.30

#: Outer vertical corner radius of the plate: the lip's 1.75 mm inner corner
#: minus clearance, rounded down to 1.40 mm for extra corner clearance.
LID_CORNER_RADIUS = 1.40

#: Skirt bottom chamfer for self-alignment (standard section 4.4).
LID_SKIRT_CHAMFER = 0.6

#: Plate top edge roundover (standard section 4.4).
LID_TOP_ROUNDOVER = 1.0

#: Finger hole diameter and its centre's distance from the +y edge.
FINGER_HOLE_DIAMETER = 20.0
FINGER_HOLE_EDGE_DISTANCE = 14.0

#: How far the skirt intrudes below the lip bottom (standard section 4.2).
SKIRT_INTRUSION = 1.0

#: Optional friction rib (standard section 4.4/6): a 0.15 mm interference
#: rib, 10 mm long and 1.5 mm tall, fused 1.2 mm into the 2.0 mm skirt wall.
LID_RIB_PROUD = 0.15
LID_RIB_LENGTH = 10.0
LID_RIB_HEIGHT = 1.5
LID_RIB_DEPTH = 1.2

#: The ribs start this far above the lip bottom, clearing the 0.6 mm skirt
#: chamfer whose top edge is 0.4 mm above the skirt bottom.
LID_RIB_BOTTOM_REL = -0.35

#: The lip's vertical-section inner offset, derived from the pinned profile
#: components (``LIP_TOP_LEDGE + LIP_TOP_CHAMFER``): the plate clearance
#: reference, never re-typed as a literal.
LIP_VERTICAL_OFFSET = gf.LIP_TOP_LEDGE + gf.LIP_TOP_CHAMFER

#: Minimum corner radius the base zone keeps (ADR-0007); the skirt's inner
#: corner radius is clamped to it like every other hidden cutter.
_MIN_RADIUS = 0.4

#: Interior cutter points used by the 0.01 mm overlap into the plate.
_PLATE_OVERLAP = 0.01


def _acceptable(shape: Part.Shape) -> bool:
    """True when ``shape`` is one valid closed solid with positive volume.

    A boolean decision, not a guard: :func:`_roundover` uses it to keep or
    reject an optional fillet result.
    """
    return (
        not shape.isNull()
        and shape.isValid()
        and shape.Volume > 0.0
        and len(shape.Solids) == 1
        and len(shape.Shells) == 1
        and shape.Shells[0].isClosed()
    )


def _require_lid(shape: Part.Shape, what: str) -> Part.Shape:
    """Return ``shape`` unwrapped, or raise :class:`GeometryError`.

    The shared one-solid/closed-shell guard plus the lid's own validity
    requirement: ``makeFillet`` and some booleans can return a
    self-intersecting solid that still reports positive volume.
    """
    solid = require_one_solid(shape, what, closed=True)
    if not solid.isValid():
        raise GeometryError(
            f"{what}: not one valid closed solid with positive volume",
        )
    return solid


def _fuse(shape: Part.Shape, other: Part.Shape, what: str) -> Part.Shape:
    return _require_lid(shape.fuse(other), what)


def _cut(shape: Part.Shape, other: Part.Shape, what: str) -> Part.Shape:
    return _require_lid(shape.cut(other), what)


def top_roundover_applied(shape: Part.Shape, height_u: int) -> bool:
    """True when the plate top edge carries the 1.0 mm roundover.

    Detected from the built shape: with the roundover applied, the plate's
    widest planar face at ``z_top`` is inset by the roundover radius from the
    lid's outer width (the plate is the widest part of the lid, flush with
    the skirt collar). This reports what was actually applied, because
    :func:`build_lid` falls back to a sharp plate when the limit fillet is
    rejected.
    """
    z_top = height_u * HEIGHT_UNIT + SKIRT_INTRUSION + LID_PLATE
    insets = [
        shape.BoundBox.XLength - face.BoundBox.XLength
        for face in shape.Faces
        if abs(face.BoundBox.ZMin - z_top) < 1e-6
        and abs(face.BoundBox.ZMax - z_top) < 1e-6
    ]
    return bool(insets) and max(insets) >= 2.0 * LID_TOP_ROUNDOVER - 1e-6


def _plate(units_x: int, units_y: int, z_plate_bottom: float, z_top: float) -> Part.Shape:
    """The 2.0 mm plate, ``LID_CLEARANCE`` clear of the lip's vertical section."""
    width = gf.outer_width(units_x) - 2.0 * (LIP_VERTICAL_OFFSET + LID_CLEARANCE)
    depth = gf.outer_width(units_y) - 2.0 * (LIP_VERTICAL_OFFSET + LID_CLEARANCE)
    return _require_lid(
        gf.rounded_rect_prism(width, depth, LID_CORNER_RADIUS, z_plate_bottom, z_top),
        "lid plate",
    )


def _contour(units_x: int, units_y: int, offset: float) -> tuple[float, float, float]:
    """``(width_x, width_y, corner radius)`` for an inward surface offset."""
    return (
        gf.outer_width(units_x) - 2.0 * offset,
        gf.outer_width(units_y) - 2.0 * offset,
        max(_MIN_RADIUS, gf.OUTER_RADIUS - offset),
    )


def _ring_segment(
    units_x: int, units_y: int, z_lip: float, z0: float, z1: float,
) -> Part.Shape:
    """One straight skirt segment between two ``z`` rows of the lip profile."""
    outer0 = gf.lip_inner_offset(z0 - z_lip) + LID_CLEARANCE
    outer1 = gf.lip_inner_offset(z1 - z_lip) + LID_CLEARANCE
    outer = gf.rounded_rect_loft(
        z0, *_contour(units_x, units_y, outer0), z1, *_contour(units_x, units_y, outer1),
    )
    inner = gf.rounded_rect_loft(
        z0,
        *_contour(units_x, units_y, outer0 + LID_SKIRT),
        z1,
        *_contour(units_x, units_y, outer1 + LID_SKIRT),
    )
    return _require_lid(outer.cut(inner), f"lid skirt segment {z0}..{z1}")


def _skirt(units_x: int, units_y: int, z_lip: float, z_skirt_bottom: float,
           z_plate_bottom: float) -> Part.Shape:
    """The 2.0 mm skirt ring, a stack of monotonic profile segments.

    One multi-row loft through the lip's non-monotonic inner profile
    self-intersects under OCCT 7.8 (negative volume while ``isValid()`` stays
    true), so the ring is built and fused segment by segment.
    """
    rows = [
        z_skirt_bottom,
        z_lip,
        z_lip + gf.LIP_BOTTOM_CHAMFER,
        z_plate_bottom + _PLATE_OVERLAP,
    ]
    ring = _ring_segment(units_x, units_y, z_lip, rows[0], rows[1])
    for z0, z1 in zip(rows[1:], rows[2:]):
        ring = _fuse(ring, _ring_segment(units_x, units_y, z_lip, z0, z1), f"lid skirt {z0}..{z1}")
    return ring


def _rib_boxes(units_x: int, z_lip: float) -> list[Part.Shape]:
    """The two friction ribs as boxes to fuse onto the +x and -x skirt faces.

    The pilot face is the skirt's outer face at the lip neck:
    ``outer_width(units_x) / 2 - (gf.lip_inner_offset(0.0) + LID_CLEARANCE)``,
    derived from the pinned profile. The rib tips sit ``LID_RIB_PROUD``
    outside it, so the ribs halve the ``LID_CLEARANCE`` to the lip neck
    locally and stay clear of the lip. The rib top disappears inside the
    plate (the plate face is flush with the skirt collar), so the visible rib
    is the band next to the lip neck.
    """
    half = gf.outer_width(units_x) / 2.0
    pilot_offset = gf.lip_inner_offset(0.0) + LID_CLEARANCE
    tip = half - pilot_offset + LID_RIB_PROUD
    inner = half - pilot_offset - (LID_RIB_DEPTH - LID_RIB_PROUD)
    z0 = z_lip + LID_RIB_BOTTOM_REL
    half_length = LID_RIB_LENGTH / 2.0

    boxes = []
    for side in (1.0, -1.0):
        origin_x = min(side * tip, side * inner)
        boxes.append(
            Part.makeBox(
                abs(tip - inner),
                LID_RIB_LENGTH,
                LID_RIB_HEIGHT,
                App.Vector(origin_x, -half_length, z0),
            ),
        )
    return boxes


def _roundover(shape: Part.Shape, z_top: float) -> Part.Shape:
    """Apply the 1.0 mm plate top roundover, or return ``shape`` unchanged.

    The roundover covers the plate top outer boundary and the finger hole's
    top rim (both edges lie at ``z_top``). A 1.0 mm roundover on a 2.0 mm
    plate is at the geometric limit; a rejected fillet -- exception or
    unacceptable result -- is not an error here, because the lid is still
    valid and printable. ``top_roundover_applied`` reports the outcome.
    """
    edges = edges_at_z(shape, z_top)
    if not edges:
        return shape
    try:
        rounded = shape.makeFillet(LID_TOP_ROUNDOVER, edges)
    except Exception:  # noqa: BLE001 - Part.OCCError is generic
        return shape
    return rounded if _acceptable(rounded) else shape


def build_lid(spec: BodySpec, ribs: bool = False) -> Part.Shape:
    """Build the inset friction lid for a body of ``spec``.

    The lid is centred in X/Y, ``z = 0`` is the body's bed face, and all
    dimensions are millimetres: the lip bottom is ``height_u * 7``,
    ``z_top = z_lip + SKIRT_INTRUSION + LID_PLATE``,
    ``z_plate_bottom = z_top - 2.0``, and
    ``z_skirt_bottom = z_lip - SKIRT_INTRUSION``. Rectangular footprints use
    ``outer_width`` per axis, so a 2 x 4 lid is not square. ``ribs=True``
    adds the standard's optional pair of 0.15 mm friction ribs. Returns one
    valid closed solid.
    """
    if spec.units_x < 1 or spec.units_y < 1:
        raise GeometryError(
            f"footprint must be at least 1 x 1, got {spec.units_x} x {spec.units_y}"
        )
    if spec.height_u < 3:
        raise GeometryError(f"height must be at least 3U, got {spec.height_u}U")

    total_h = spec.height_u * HEIGHT_UNIT + gf.LIP_HEIGHT
    z_lip = total_h - gf.LIP_HEIGHT
    # Keep the SYS-1.0 lid shape relative to the lip bottom: plate bottom
    # 1.0 mm above the lip bottom, skirt bottom 1.0 mm below it. With the
    # official 4.4 mm lip the lid top is therefore 1.4 mm below the lip top.
    z_top = z_lip + SKIRT_INTRUSION + LID_PLATE
    z_plate_bottom = z_top - LID_PLATE
    z_skirt_bottom = z_lip - SKIRT_INTRUSION
    if z_skirt_bottom <= 0.0:
        raise GeometryError(
            f"{spec.height_u}U leaves no room for the lid skirt below the lip bottom",
        )

    lid = _fuse(
        _plate(spec.units_x, spec.units_y, z_plate_bottom, z_top),
        _skirt(spec.units_x, spec.units_y, z_lip, z_skirt_bottom, z_plate_bottom),
        "lid plate and skirt",
    )

    bottom_edges = edges_at_z(lid, z_skirt_bottom)
    if not bottom_edges:
        raise GeometryError("lid skirt bottom chamfer: no edges at the skirt bottom")
    lid = _require_lid(lid.makeChamfer(LID_SKIRT_CHAMFER, bottom_edges), "lid chamfer")

    lid = _roundover(lid, z_top)

    hole = Part.makeCylinder(
        FINGER_HOLE_DIAMETER / 2.0,
        LID_PLATE + 2.0,
        App.Vector(
            0.0,
            gf.outer_width(spec.units_y) / 2.0 - FINGER_HOLE_EDGE_DISTANCE,
            z_plate_bottom - 1.0,
        ),
        App.Vector(0.0, 0.0, 1.0),
    )
    lid = _cut(lid, hole, "lid finger hole")

    if ribs:
        for index, rib in enumerate(_rib_boxes(spec.units_x, z_lip)):
            lid = _fuse(lid, rib, f"lid rib {index}")

    return _require_lid(lid, "lid")
