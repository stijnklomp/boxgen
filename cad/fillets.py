"""Internal fillets with the SYS-1.0 reduce-then-fail contract.

The standard (section 4.1) requires every internal corner above the base to
be filleted: apply the 2.0 mm radius, reduce to 1.0 mm when that cannot be
applied, and raise a geometry error when even the fallback fails. A sharp
internal corner above the base is never acceptable.

FreeCAD 1.0.0 (OCCT 7.8) can also "succeed" at ``makeFillet`` while returning
an invalid shape -- the API notes recorded a 1.5 mm fillet on a fused divider
junction that returned ``isValid() == False`` -- so a candidate is accepted
only when it passes all three checks:

* ``isValid()`` is true,
* it contains exactly one solid,
* its volume is positive.

``makeFillet`` on a fused shell fails outright (``Part.OCCError``) for the
floor-to-wall cove; the shell builder pre-fillets the interior cutter
instead, which is the verified recipe in the FreeCAD API notes.

This module imports only ``Part`` and is used by :mod:`cad.shell`; the
``GeometryError`` raised here is defined in :mod:`cad.geometry` and
re-exported for the callers that catch it, and it is mapped to the build's
geometry failure (exit code 3).
"""

from __future__ import annotations

import Part

from .geometry import GeometryError

__all__ = ["FILLET_FALLBACK", "GeometryError", "fillet_with_fallback"]

#: The standard's fallback radius when the 2.0 mm internal fillet cannot be
#: applied (design standard section 4.1).
FILLET_FALLBACK = 1.0


def _acceptable(candidate: Part.Shape) -> bool:
    """True when a fillet result is a usable solid.

    ``makeFillet`` can return an invalid shape without raising, so the three
    invariants are checked explicitly.
    """
    return (
        not candidate.isNull()
        and candidate.isValid()
        and len(candidate.Solids) == 1
        and candidate.Volume > 0.0
    )


def _single_solid(shape: Part.Shape) -> Part.Shape:
    """Unwrap a boolean-style compound (one solid) to the solid itself."""
    return shape.Solids[0] if shape.ShapeType == "Compound" else shape


def fillet_with_fallback(
    shape: Part.Shape,
    radius: float,
    edges: list[Part.Edge],
    fallback: float = FILLET_FALLBACK,
    what: str = "fillet",
) -> tuple[Part.Shape, float]:
    """Fillet ``edges`` with ``radius``, reducing to ``fallback`` if needed.

    Returns ``(filleted_shape, radius_used)`` so the caller can record the
    radius actually applied. Raises :class:`GeometryError` when neither the
    standard radius nor the fallback produces an acceptable shape; a caller
    must not fall back to the sharp-cornered input.
    """
    if not edges:
        raise GeometryError(f"{what}: no edges to fillet")
    if radius <= 0.0 or fallback <= 0.0:
        raise GeometryError(
            f"{what}: fillet radii must be positive, got {radius} and {fallback}",
        )

    failures = []
    for candidate_radius in (radius, fallback):
        try:
            candidate = shape.makeFillet(candidate_radius, edges)
        except Exception as error:  # noqa: BLE001 - Part.OCCError is generic
            failures.append(f"{candidate_radius} mm: {error}")
            continue
        if not _acceptable(candidate):
            failures.append(f"{candidate_radius} mm: invalid result")
            continue
        return _single_solid(candidate), candidate_radius

    raise GeometryError(
        f"{what} failed at both {radius} mm and {fallback} mm and the body "
        "would keep a sharp internal corner: " + "; ".join(failures)
    )
