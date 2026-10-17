# syntax=docker/dockerfile:1

# Docker-first development and gate image for boxgen.
#
# The host needs no Rust, cargo, or just: mount the checkout at /app, put the
# cargo home and target directory in /tmp, and run `just check`. FreeCAD is not
# part of this image; ticket #3 adds the pinned FreeCAD toolchain image.
FROM rust:1.99.1-slim-trixie

# `just` 1.40.0 is the Debian trixie package. ca-certificates lets cargo fetch
# crates; the base image already carries git for registry access.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates just \
    && rm -rf /var/lib/apt/lists/*

# rust-toolchain.toml pins channel 1.99.1 with clippy and rustfmt. Install the
# gate components once at image build time so container runs need no rustup
# downloads.
RUN rustup component add clippy rustfmt

WORKDIR /app
