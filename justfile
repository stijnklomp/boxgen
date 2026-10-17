# Command surface for boxgen. Run `just` for the recipe list.
#
# The host needs no Rust toolchain: build the dev image once with
# `just docker-build-dev` (or the documented `docker build` command) and run
# every recipe inside it. The documented gate is `just check`.

# List the available recipes.
default:
    @just --list

# Fast gate: formatting, lint with warnings denied, and the test suite.
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

# Run the test suite (unit, integration, and doctests).
test:
    cargo test

# Run the CLI, for example `just run -- --help`.
run *args:
    cargo run -- {{args}}

# Build the Docker-first development and gate image.
docker-build-dev:
    docker build -f docker/dev.Dockerfile -t boxgen/dev:rust-1.99.1 .

# Run the full gate in a fresh dev container. This is the command documented in
# README.md and AGENTS.md; it needs only Docker on the host.
docker-check:
    docker run --rm -u "$$(id -u):$$(id -g)" -e CARGO_HOME=/tmp/cargo -e CARGO_TARGET_DIR=/tmp/target -v "$$PWD":/app -w /app boxgen/dev:rust-1.99.1 just check
