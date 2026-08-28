# Convenience recipes around the workspace's existing cargo/make commands.
# `just` is optional -- every recipe here is a thin wrapper, not new tooling.

# Running `just` with no recipe name shows this list (matches `just help`).
default: help

# List available recipes.
help:
    @just --list

# Type-check every workspace member without building artifacts.
check:
    cargo check --workspace

# Run the workspace's unit and integration tests.
test:
    cargo test --workspace

# Check formatting without writing changes.
fmt-check:
    cargo fmt --all -- --check

# Apply formatting.
fmt:
    cargo fmt --all

# Lint with clippy across the workspace.
clippy:
    cargo clippy --workspace

# Build every contract's release WASM -- see Makefile's `wasm` target.
wasm:
    @make wasm

# Regenerate TypeScript client bindings from each contract's WASM -- see
# Makefile's `bindings` target.
bindings:
    @make bindings

# One-command build: WASM plus its bindings -- see Makefile's `build` target.
build:
    @make build
