# Command surface for boxgen. Run `just` for the recipe list.
#
# Two gates: `just check` is the native Rust-only gate; `just check-docker`
# builds the pinned FreeCAD image and runs the full gate inside it.

# The FreeCAD release pinned by the toolchain image. This is the one line to
# bump when upgrading the geometry toolchain; the image tag follows it.
freecad_version := "1.0.0"

freecad_image := "boxgen/freecad:" + freecad_version

# List the available recipes.
default:
    @just --list

# Fast native gate: formatting, lint with warnings denied, and tests.
check: fmt-check lint test

# Format the crate.
fmt:
    cargo fmt --all

# Check formatting without writing.
fmt-check:
    cargo fmt --all --check

# Lint with warnings denied.
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Build the CLI.
build:
    cargo build

# The recipe passes the pinned FreeCAD release to the toolchain test; on a
# native host without FreeCAD that test prints a skip and passes.
#
# Run the Rust test suite (unit, integration, and doctests).
test:
    BOXGEN_EXPECT_FREECAD_VERSION={{freecad_version}} cargo test

# Run the CLI, for example `just run -- --help`.
run *args:
    cargo run -- {{args}}

# Build the pinned FreeCAD gate image.
docker-build:
    docker build --build-arg FREECAD_VERSION={{freecad_version}} -f "{{justfile_directory()}}/docker/freecad.Dockerfile" -t {{freecad_image}} "{{justfile_directory()}}"

# fmt-check, lint, tests, and the FreeCAD toolchain assertion under the same
# pinned release. This is the authoritative gate and the command CI runs.
#
# Run the full gate inside the pinned image.
check-docker: docker-build
    docker run --rm -u "$(id -u):$(id -g)" \
        -e HOME=/tmp/boxgen-home \
        -e FREECAD_USER_HOME=/tmp/boxgen-freecad \
        -e CARGO_HOME=/tmp/boxgen-cargo \
        -e CARGO_TARGET_DIR=/tmp/boxgen-target \
        -e BOXGEN_REQUIRE_FREECAD=1 \
        -v "{{justfile_directory()}}":/app -w /app \
        {{freecad_image}} just check

# Smoke-test the pinned image: version plus a trivial Part solid.
freecad-smoke: docker-build
    docker run --rm {{freecad_image}} sh -c 'freecadcmd --version && freecadcmd -c "import FreeCAD, Part; assert abs(Part.makeBox(1, 2, 3).Volume - 6) < 1e-9; print(\"freecadcmd smoke ok\")"'
