"""Shared shape invariants for the boxgen master.

Every boolean and construction step in the master must leave exactly one
solid with positive volume, and the finished body and lid must be one closed
shell (watertight). :func:`require_one_solid` is that guard, shared by
:mod:`cad.shell`, :mod:`cad.dividers`, and :mod:`cad.lid`. The vendored
:mod:`cad.gridfinity` keeps its own ``_single_solid`` because it must stay
importable standalone (``FreeCAD`` and ``Part`` only; see its module
docstring), and :mod:`cad.fillets` keeps its boolean acceptance predicate for
the reduce-then-fail fillet rule.

:class:`GeometryError` is defined here and re-exported by :mod:`cad.fillets`,
the module that owns the standard's fillet contract; it maps to the build's
geometry failure (exit code 3).
"""

from __future__ import annotations

import Part

__all__ = ["GeometryError", "require_one_solid"]


class GeometryError(RuntimeError):
    """A body that cannot be built to the SYS-1.0 invariants."""


def require_one_solid(
    shape: Part.Shape, what: str, *, closed: bool = False
) -> Part.Shape:
    """Return ``shape`` as one positive solid, or raise :class:`GeometryError`.

    ``what`` names the construction step for the error message. With
    ``closed=True`` the solid must also be one closed shell (watertight),
    which the finished body and lid require.

    Boolean results come back as a compound wrapping one solid, so the
    compound is unwrapped; a loft can silently self-intersect with a negative
    volume, so the invariants are checked at every construction step.
    """
    if shape.isNull():
        raise GeometryError(f"{what}: empty shape")
    if shape.Volume <= 0.0:
        raise GeometryError(f"{what}: non-positive volume {shape.Volume}")
    solids = shape.Solids
    if len(solids) != 1:
        raise GeometryError(f"{what}: expected one solid, got {len(solids)}")

    solid = solids[0] if shape.ShapeType == "Compound" else shape
    if closed:
        shells = solid.Shells
        if len(shells) != 1 or not shells[0].isClosed():
            raise GeometryError(
                f"{what}: expected one closed shell, got {len(shells)}"
            )
    return solid
