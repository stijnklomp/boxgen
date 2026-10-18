# syntax=docker/dockerfile:1

# The one-command local UI image (v2, ticket #20, ADR-0002): `boxgen serve`
# on the pinned FreeCAD toolchain. Built and run by `scripts/boxgen-ui.sh`
# (`just ui`); the base image comes from docker/freecad.Dockerfile.
#
# The pinned FreeCAD release of the base image. The default mirrors
# `docker/freecad.Dockerfile`'s `FREECAD_VERSION` (1.0.0) so a direct
# `docker build -f docker/ui.Dockerfile .` works; the justfile's
# `freecad_version` is the single source, and `scripts/boxgen-ui.sh` always
# passes it as this build arg (building the base image first when missing).
# Declared before the first FROM so the second FROM can use it.
ARG FREECAD_VERSION=1.0.0

# Stage 1 compiles the release binary with the pinned Rust toolchain. The
# WORKDIR is /app on purpose: `src/build.rs::repo_cad_dir()` bakes the
# compile-time `CARGO_MANIFEST_DIR` into the binary as the location of the
# FreeCAD master `cad/`. At runtime the launcher mounts the host workspace at
# /app, so the baked path must resolve to the mounted repository's `cad/`;
# compiling the source anywhere else would make every build inside the UI
# container fail with a missing master.
FROM rust:1.99.1-slim-trixie AS builder

WORKDIR /app

# rust-toolchain.toml requests clippy and rustfmt; sync them at image build
# time (the network is available here) so the compile layer never stops to
# fetch rustup components. The tag above pins the same 1.99.1 release.
COPY rust-toolchain.toml ./
RUN rustup component add clippy rustfmt

# Only what the compile needs: the manifests, the crate sources, and the
# frontend assets embedded with `include_bytes!`. `--locked` keeps the build
# on the committed Cargo.lock.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY assets ./assets

RUN cargo build --locked --release

# Stage 2: the pinned FreeCAD gate image (`boxgen/freecad:<pin>`) plus the
# compiled binary.
FROM boxgen/freecad:${FREECAD_VERSION}

ARG FREECAD_VERSION
LABEL org.opencontainers.image.title="boxgen local UI" \
      org.opencontainers.image.description="boxgen serve on the pinned FreeCAD ${FREECAD_VERSION} toolchain" \
      org.opencontainers.image.version="${FREECAD_VERSION}"

COPY --from=builder /app/target/release/boxgen /usr/local/bin/boxgen

WORKDIR /app
