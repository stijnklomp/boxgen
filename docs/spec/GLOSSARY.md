# Stackable Storage System

A 3D-printed storage system for household equipment: Gridfinity-compatible bodies with integral compartments, optional friction lids, and a maximum-thickness shell designed for the Bambu Lab X2D in PETG Basic.

## Parts

**Body**:
A printed container comprising a Gridfinity base zone, a body zone, a floor, a lip zone, and integral dividers.
_Avoid_: box, shell, bin.

**Lid**:
A removable friction-fit plate that closes a body.
_Avoid_: cover, top.

**Case**:
A body with its lid fitted.
_Avoid_: box.

**Open bin**:
A body used without a lid.
_Avoid_: tray, bucket.

**Compartment**:
A region inside a body bounded by the outer wall and dividers; a leaf of the division tree.
_Avoid_: cell, slot, section.

**Divider**:
An integral wall that separates compartments.
_Avoid_: partition, insert.

## Geometry and sizing

**Base zone**:
The bottom section of a body containing the standard Gridfinity base profile.
_Avoid_: foot, bottom, pedestal.

**Body zone**:
The vertical wall section between the base zone and the lip zone; it carries the stack load.
_Avoid_: wall, side.

**Lip zone**:
The top section of a body containing the standard Gridfinity stacking lip.
_Avoid_: rim, edge, collar.

**Floor**:
The horizontal slab at the top of the base zone on which contents rest.

**Interior depth**:
The usable vertical distance from the floor to the bottom of the lip: `U × 7 − 5.55 mm`. The lip is above the body and does not reduce it.

**Footprint**:
A body's horizontal size in Gridfinity units, written N × M (for example 4 × 4); maximum 4 × 4.
_Avoid_: size, dimensions.

**Module**:
The 4 × 4 footprint that any smaller body tiles; the reference size of the system.
_Avoid_: plate, base.

**Height**:
A body's nominal vertical size in Gridfinity units of 7 mm, written U (for example 6U); 6U is the default. The body height runs from the base bottom to the nominal top plane (the lip bottom); the official 4.4 mm lip sits above it.
_Avoid_: depth, tallness.

**Division tree**:
The recursive structure that defines a body's compartments: each compartment is either a leaf or a split into two children.
_Avoid_: compartment map, layout, partition plan.

**Split**:
A division of one compartment into two children along the x axis (vertical divider, moves left/right) or the y axis (horizontal divider, moves up/down).

**Position**:
The distance in integer millimetres from a compartment's origin edge to the near face of its new divider.
_Avoid_: offset, coordinate.

**Leaf**:
A compartment that is not split further.

**Divider height**:
The height of a divider above the floor; interior depth minus 10 mm, or interior depth minus 2 mm on 3U bodies.

**Stack group**:
Bodies with the same footprint that may be stacked together.
_Avoid_: stack, column.

**Stack pitch**:
The vertical distance from one body's base bottom to the next body's base bottom in a stack: `U × 7 − 0.35 mm`. The 0.35 mm is the measured base seat below the nominal top plane. Lids are top-only and add no pitch.

**Stack height**:
The outer height of a stack of n bodies: `Σ(U × 7) + 4.4 − (n−1) × 0.35 mm`. Two 6U bodies stack to 88.05 mm; a single 12U is 88.4 mm.

**Legacy body**:
A SYS-1.0 body with a 4.0 mm workbench lip and the lip counted inside `U × 7`. It is mechanically stackable with other legacy bodies but is not a SYS-2.0 height class. Its lids fit SYS-2.0 bodies because the lid shape relative to the lip bottom is unchanged.

**Stack limit**:
The maximum number of bodies in a stack and the maximum contents weight per body; three bodies and 10 kg each.

**Corner radius**:
A rounded corner or junction that removes a stress concentration; outer 3.75 mm, inner 1.75 mm, joints 2.0 mm.

## Process

**Spec**:
A YAML file that defines one body: footprint, height, division tree, lid, and notes.

**Generator**:
The Rust CLI that validates a spec and emits a FreeCAD Python build script.
_Avoid_: app, tool, box maker.

**Master**:
The FreeCAD Python module that implements the fixed body architecture and the pinned Gridfinity profiles.
_Avoid_: template, source file.

**Coupon**:
A small calibration print that validates the base, lip, lid fit, shell thickness, corner radii, and divider joint before production boxes are printed.
_Avoid_: test print, calibration piece.

**Print recipe**:
The material, nozzle, layer, wall, infill, and temperature settings used for every body and coupon.

**Fit class**:
A named clearance between two printed surfaces, such as the stack fit or the lid fit.

**Acceptance criteria**:
The measurable conditions a coupon must meet before a production print.

**Duty class**:
The load and use rating of the system; this system uses the heavy-duty class.

**System version**:
`SYS-<major>.<minor>` identifies the fixed architecture; a major bump invalidates previously printed bodies.

## Standards and tools

**Gridfinity**:
The open 42 × 42 × 7 mm modular storage standard created by Zack Freedman and released under the MIT licence.
_Avoid_: grid system, modular grid.

**Bento3D**:
A proprietary web-based box generator; it is a UX reference for the division editor, not this system's source of truth.
_Avoid_: box designer.

**FreeCAD**:
The open-source CAD application used to build the master and generated models.

**Bambu Studio**:
The slicer used to turn exported models into print files.

**X2D**:
The Bambu Lab printer used for all production prints.
