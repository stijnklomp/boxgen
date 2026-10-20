# Pin the Gridfinity profile geometry in the master

The master vendors the Gridfinity base and lip profile functions (from the FreeCAD Gridfinity Workbench, LGPL-2.1, attribution kept in the file header) instead of calling a separately installed workbench version. The base and lip are the interface that must never drift; pinning the functions makes every build reproducible and prevents a workbench update from silently changing the fit. The workbench remains the reference for the original profile dimensions.

**SYS-2.0 note:** the lip half of this decision is superseded by [ADR-0015](0015-official-gridfinity-height-and-lip.md). The base profile and rounded-rectangle construction remain pinned from the workbench; the lip is the official 4.4 mm profile.
