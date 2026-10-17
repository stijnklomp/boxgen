# syntax=docker/dockerfile:1

# Pinned FreeCAD toolchain and gate image for boxgen.
#
# This is the authoritative gate image: Rust 1.99.1 (from rust-toolchain.toml),
# `just`, and the headless FreeCAD launcher at the release pinned in the image
# tag. The `justfile` builds it as `boxgen/freecad:<freecad_version>`; bump the
# pin there and the tag changes with it.
FROM rust:1.99.1-slim-trixie

# FreeCAD release installed from Debian trixie. `just docker-build` and CI pass
# this from the single `freecad_version` pin in the justfile; the default keeps
# a direct `docker build -f docker/freecad.Dockerfile .` usable.
ARG FREECAD_VERSION=1.0.0

LABEL org.opencontainers.image.title="boxgen pinned FreeCAD toolchain" \
      org.opencontainers.image.description="Rust 1.99.1 + just + headless FreeCAD ${FREECAD_VERSION} for the boxgen gate" \
      org.opencontainers.image.version="${FREECAD_VERSION}"

# Debian trixie ships FreeCAD 1.0.0 as `freecad-python3`. The wildcard pins the
# upstream release while allowing Debian revision bumps (1.0.0+dfsg-8+deb13u3).
# GUI packages are deliberately not installed: `freecadcmd` is the only
# launcher the generator uses.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        "freecad-python3=${FREECAD_VERSION}*" \
        just \
    && rm -rf /var/lib/apt/lists/*

# rust-toolchain.toml requests clippy and rustfmt; install them at image build
# time so container runs need no rustup downloads.
RUN rustup component add clippy rustfmt

# Fail the build if the package does not provide the pinned release or if the
# headless launcher cannot import the geometry kernel. This keeps the tag and
# the image content honest; `just freecad-smoke` repeats the check on demand.
RUN test "$(freecadcmd --version 2>&1 | awk '{print $2}')" = "${FREECAD_VERSION}" \
    && freecadcmd -c "import FreeCAD, Part; assert '.'.join(FreeCAD.Version()[0:3]) == '${FREECAD_VERSION}', FreeCAD.Version(); assert abs(Part.makeBox(1, 2, 3).Volume - 6.0) < 1e-9"

# The gate runs as the host UID, not root, and keeps all mutable state in /tmp.
# FreeCAD 1.0.0 segfaults instead of creating a missing user home, so create
# the documented paths world-writable up front. They are throw-away state.
RUN mkdir -p /tmp/boxgen-home /tmp/boxgen-freecad /tmp/boxgen-cargo /tmp/boxgen-target \
    && chmod 1777 /tmp/boxgen-home /tmp/boxgen-freecad /tmp/boxgen-cargo /tmp/boxgen-target

WORKDIR /app
