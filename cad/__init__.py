"""boxgen FreeCAD master package (SYS-1.0).

The generated ``box.py`` imports this package from the build directory. The
modules use only ``FreeCAD`` and ``Part`` and run headless under
``freecadcmd``; no GUI, Sketcher, PartDesign, or installed workbench is used.
"""

__all__ = ["build", "fillets", "gridfinity", "shell"]
