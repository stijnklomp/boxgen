# Bento3D is not the source of truth

Bento3D is a closed-source web application built on the open-source Nodi engine; the application itself is not published, cannot be self-hosted, and exports STL rather than a parametric model. We keep Bento3D only as a UX reference for the compartment-map editor. The source of truth is the local FreeCAD master plus per-box specs. This avoids a website dependency and keeps the design reproducible for the life of the system.

**Consequences:** the compartment-map editor and generator are part of this project, not an external service. The design must remain usable with FreeCAD alone if the generator is unavailable.
