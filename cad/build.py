"""FreeCAD master entry point for boxgen.

``build(spec, out_dir)`` builds the body and, when the spec has ``lid: true``,
the inset friction lid, writes ``box.step``, ``box.FCStd``, ``body.stl``,
``lid.stl``, and returns the report dict. The generated ``box.py`` writes that
dict to ``report.json`` with sorted keys; the Rust build then adds the
validation warnings and rewrites it deterministically.

A division tree with splits is built through :mod:`cad.dividers`: every split
becomes a 2.0 mm plate fused into the shell with its floor and junction coves
and 0.4 mm top chamfer. A single leaf adds no divider material, so the
single-compartment path is unchanged.

The report records the floor-to-wall fillet radius actually applied in
``body.floor_fillet_mm`` (2.0 mm, or the 1.0 mm fallback), the resolved
divider rectangles in ``dividers``, the usable interior depth measured from
the built body (standard §4.2: floor top to lip bottom; the design formula
``U * 7 - 8.75`` stays the reference in :mod:`cad.shell`), and the lid's
actual measurements in ``lid`` (including the top roundover that was applied
and whether the friction ribs are present), adding a deterministic warning
when a standard radius had to be reduced. The radii and the reduce-then-fail
rule live in :mod:`cad.fillets`, :mod:`cad.shell`, :mod:`cad.dividers`, and
:mod:`cad.lid`.

The calibration coupon (ticket #10) is the one internal spec with
``coupon: true``: instead of the division tree it fuses the single 2.0 mm
T-divider from :func:`cad.dividers.build_coupon_divider` into the shell and
records it in the report's ``coupon`` section. The closed user schema never
accepts the flag; only ``boxgen coupon`` sets it.

Mass and print-time estimates use the measured body and lid volume with the
same documented constants as ``src/tree.rs``: PETG 1.27 g/cm3 and 5.5 mm3/s
effective flow. Using the built volume is more accurate than the analytic
estimate and keeps ``report.json`` a description of the geometry that was
actually produced.
"""

from __future__ import annotations

import os

import FreeCAD as App
import Import
import MeshPart

from . import dividers as dividers_module
from . import gridfinity as gf
from . import lid as lid_module
from . import shell

__all__ = ["build"]

#: PETG Basic density, mirrored from ``tree::PETG_DENSITY_G_PER_CM3``.
PETG_DENSITY_G_PER_CM3 = 1.27

#: Conservative effective extrusion throughput, mirrored from
#: ``tree::EFFECTIVE_FLOW_MM3_PER_S``.
EFFECTIVE_FLOW_MM3_PER_S = 5.5

#: STL tessellation. A 0.05 mm chord error is invisible at a 0.4 mm nozzle
#: and produces a watertight, deterministic binary mesh.
MESH_LINEAR_DEFLECTION = 0.05
MESH_ANGULAR_DEFLECTION = 0.3


def _round(value: float, digits: int = 6) -> float:
    return round(float(value), digits)


def _bbox(shape) -> dict[str, float]:
    box = shape.BoundBox
    return {
        "x_min": _round(box.XMin),
        "x_max": _round(box.XMax),
        "y_min": _round(box.YMin),
        "y_max": _round(box.YMax),
        "z_min": _round(box.ZMin),
        "z_max": _round(box.ZMax),
    }


def _check_spec(spec: dict) -> tuple[shell.BodySpec, bool, bool, bool]:
    """Validate what the master needs.

    Returns ``(body, lid, lid_ribs, coupon)`` where ``body`` is the
    :class:`cad.shell.BodySpec` holding the footprint and height.
    """
    if not isinstance(spec, dict):
        raise shell.GeometryError("spec must be a mapping")
    if not spec.get("id"):
        raise shell.GeometryError("spec.id is required")

    lid = spec.get("lid")
    if not isinstance(lid, bool):
        raise shell.GeometryError("spec.lid must be a boolean")
    lid_ribs = spec.get("lid_ribs", False)
    if not isinstance(lid_ribs, bool):
        raise shell.GeometryError("spec.lid_ribs must be a boolean")
    if lid_ribs and not lid:
        raise shell.GeometryError(
            "lid_ribs needs a lid; set `lid: true` or `lid_ribs: false`"
        )

    coupon = spec.get("coupon", False)
    if not isinstance(coupon, bool):
        raise shell.GeometryError("spec.coupon must be a boolean")

    divisions = spec.get("divisions")
    if not isinstance(divisions, dict):
        raise shell.GeometryError("spec.divisions must be a mapping")

    footprint = spec.get("footprint")
    if not isinstance(footprint, (list, tuple)) or len(footprint) != 2:
        raise shell.GeometryError("spec.footprint must be a [units_x, units_y] pair")
    units_x, units_y = int(footprint[0]), int(footprint[1])

    height_u = int(spec.get("height_u", 0))

    if coupon and (units_x, units_y, height_u, lid, lid_ribs) != (1, 1, 6, True, True):
        raise shell.GeometryError(
            "the coupon is fixed at 1 x 1 x 6U with the ribbed lid "
            "(calibration section 2)"
        )

    body_spec = shell.BodySpec(units_x=units_x, units_y=units_y, height_u=height_u)
    return body_spec, lid, lid_ribs, coupon


def _remove_backups(out_dir: str) -> None:
    """Drop the timestamped ``.FCBak`` that ``doc.saveAs`` creates."""
    try:
        names = os.listdir(out_dir)
    except OSError:
        return
    for name in names:
        if name.startswith("box.") and name.endswith(".FCBak"):
            try:
                os.remove(os.path.join(out_dir, name))
            except OSError:
                pass


def _lid_report(lid_shape, lid_ribs: bool, spec: shell.BodySpec) -> dict:
    """Measurements for the report's ``lid`` section (standard section 4.4)."""
    shells = lid_shape.Shells
    roundover_mm = (
        lid_module.LID_TOP_ROUNDOVER
        if lid_module.top_roundover_applied(lid_shape, spec.height_u)
        else 0.0
    )
    return {
        "volume_mm3": _round(lid_shape.Volume),
        "area_mm2": _round(lid_shape.Area),
        "faces": len(lid_shape.Faces),
        "solids": len(lid_shape.Solids),
        "is_valid": bool(lid_shape.isValid()),
        "is_closed": bool(len(shells) == 1 and shells[0].isClosed()),
        "roundover_mm": _round(roundover_mm),
        "ribs": bool(lid_ribs),
        "bbox": _bbox(lid_shape),
    }


def _dividers_report(spec: shell.BodySpec, divisions: dict) -> dict:
    """Measurements for the report's ``dividers`` section (standard 4.3).

    The plan is the resolved geometry: one entry per split with the absolute
    near and far faces and the full parent span in centred coordinates, in
    traversal order. A single-leaf tree resolves to an empty plan.
    """
    plan = dividers_module.divider_plan(spec, divisions)
    height = dividers_module.divider_height(spec.height_u)
    return {
        "count": len(plan),
        "height_above_floor_mm": _round(height),
        "top_mm": _round(gf.BASE_HEIGHT + height),
        "plan": [
            {
                "axis": divider.axis,
                "near_face_mm": _round(divider.near_face_mm),
                "far_face_mm": _round(divider.far_face_mm),
                "span_lo_mm": _round(divider.span_lo_mm),
                "span_hi_mm": _round(divider.span_hi_mm),
                "lo_end": divider.lo_end.value,
                "hi_end": divider.hi_end.value,
            }
            for divider in plan
        ],
    }


def build(spec: dict, out_dir: str) -> dict:
    """Build the body and optional lid, export STEP/FCStd/STL, return the report."""
    body_spec, lid, lid_ribs, coupon = _check_spec(spec)
    os.makedirs(out_dir, exist_ok=True)

    body_result = shell.build_body(body_spec)
    if coupon:
        body = dividers_module.build_coupon_divider(body_result.shape, body_spec)
    else:
        body = dividers_module.build_dividers(
            body_result.shape, body_spec, spec["divisions"]
        )
    lid_shape = (
        lid_module.build_lid(body_spec, ribs=lid_ribs) if lid else None
    )

    step_path = os.path.join(out_dir, "box.step")
    fcstd_path = os.path.join(out_dir, "box.FCStd")
    stl_path = os.path.join(out_dir, "body.stl")
    lid_stl_path = os.path.join(out_dir, "lid.stl")

    # Both solids live in one document and one STEP (contract section 6).
    document = App.newDocument("box")
    try:
        body_object = document.addObject("Part::Feature", "Body")
        body_object.Shape = body
        objects = [body_object]
        if lid_shape is not None:
            lid_object = document.addObject("Part::Feature", "Lid")
            lid_object.Shape = lid_shape
            objects.append(lid_object)
        document.recompute()

        Import.export(objects, step_path)
        document.saveAs(fcstd_path)
    finally:
        App.closeDocument(document.Name)

    _remove_backups(out_dir)

    mesh = MeshPart.meshFromShape(
        Shape=body,
        LinearDeflection=MESH_LINEAR_DEFLECTION,
        AngularDeflection=MESH_ANGULAR_DEFLECTION,
        Relative=False,
    )
    mesh.write(stl_path)

    lid_mesh = None
    if lid_shape is not None:
        lid_mesh = MeshPart.meshFromShape(
            Shape=lid_shape,
            LinearDeflection=MESH_LINEAR_DEFLECTION,
            AngularDeflection=MESH_ANGULAR_DEFLECTION,
            Relative=False,
        )
        lid_mesh.write(lid_stl_path)

    volume_mm3 = body.Volume + (lid_shape.Volume if lid_shape is not None else 0.0)
    shells = body.Shells
    floor_fillet_mm = float(body_result.floor_fillet_mm)
    warnings = []
    if floor_fillet_mm < shell.FLOOR_FILLET:
        warnings.append(
            "body.floor_fillet_mm: the floor-to-wall fillet was reduced from "
            f"{shell.FLOOR_FILLET:.1f} mm to {floor_fillet_mm:.1f} mm because the "
            "standard radius failed; SYS-1.0 section 4.1 reduce-then-fail rule"
        )
    report = {
        "id": spec["id"],
        "dimensions": {
            "outer_x_mm": _round(body.BoundBox.XLength),
            "outer_y_mm": _round(body.BoundBox.YLength),
            "height_mm": _round(body.BoundBox.ZLength),
        },
        "interior_depth_mm": _round(
            shell.measure_interior_depth_mm(body, body_spec.height_u)
        ),
        "dividers": _dividers_report(body_spec, spec["divisions"]),
        "body": {
            "volume_mm3": _round(body.Volume),
            "area_mm2": _round(body.Area),
            "faces": len(body.Faces),
            "solids": len(body.Solids),
            "is_valid": bool(body.isValid()),
            "is_closed": bool(len(shells) == 1 and shells[0].isClosed()),
            "bbox": _bbox(body),
            "floor_fillet_mm": _round(floor_fillet_mm),
        },
        "stl": {
            "facets": int(mesh.CountFacets),
            "is_solid": bool(mesh.isSolid()),
            "volume_mm3": _round(mesh.Volume),
        },
        "artifacts": {
            "box_step_bytes": os.path.getsize(step_path),
            "body_stl_bytes": os.path.getsize(stl_path),
        },
        "mass_g": _round(volume_mm3 / 1000.0 * PETG_DENSITY_G_PER_CM3, 2),
        "print_time_h": _round(
            volume_mm3 / EFFECTIVE_FLOW_MM3_PER_S / 3600.0, 2
        ),
        "warnings": warnings,
    }
    if coupon:
        report["coupon"] = {
            "t_divider": True,
            "wall": dividers_module.COUPON_DIVIDER_WALL,
            "length_mm": _round(dividers_module.COUPON_DIVIDER_LENGTH),
            "thickness_mm": _round(dividers_module.DIVIDER_THICKNESS),
            "height_above_floor_mm": _round(
                dividers_module.divider_height(body_spec.height_u)
            ),
            "top_mm": _round(
                gf.BASE_HEIGHT + dividers_module.divider_height(body_spec.height_u)
            ),
            "top_chamfer_mm": _round(dividers_module.DIVIDER_TOP_CHAMFER),
        }
    if lid_shape is not None:
        lid_report = _lid_report(lid_shape, lid_ribs, body_spec)
        if lid_report["roundover_mm"] < lid_module.LID_TOP_ROUNDOVER:
            warnings.append(
                "lid.roundover_mm: the 1.0 mm plate-top roundover was not applied "
                f"(report.json says {lid_report['roundover_mm']:.1f} mm) because the "
                "limit fillet failed; the lid keeps a sharp but valid top edge"
            )
        report["lid"] = lid_report
        report["lid_stl"] = {
            "facets": int(lid_mesh.CountFacets),
            "is_solid": bool(lid_mesh.isSolid()),
            "volume_mm3": _round(lid_mesh.Volume),
        }
        report["artifacts"]["lid_stl_bytes"] = os.path.getsize(lid_stl_path)
    return report
