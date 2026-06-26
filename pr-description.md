## Summary

Fail the build if a feature flag is referenced in Rust source code but not declared in the crate's `[features]` section of `Cargo.toml`.

Closes # (replace with issue number)

## Background

Previously, writing `#[cfg(feature = "some_feature")]` in source without a matching entry in `[features]` would silently compile to dead code — the cfg condition could never be true. This forced workarounds and made feature-gated code fragile.

The project already had one such instance: `remitwise-common/src/lib.rs:302` used `#[cfg(any(test, feature = "testutils"))]` but `testutils` was never declared in `remitwise-common/Cargo.toml`'s `[features]`. The `feature = "testutils"` branch was dead code.

## Changes

### Bug fix: declare the orphan feature

**`remitwise-common/Cargo.toml`** — Added:
```toml
[features]
testutils = ["soroban-sdk/testutils"]
```
This makes the `cfg(feature = "testutils")` condition meaningful by properly wiring it to the `soroban-sdk/testutils` dependency feature.

### New tool: feature flag consistency checker

**`scripts/check_features.py`** — A new Python script that:
- Walks all workspace member crates
- Reads each crate's `[features]` from `Cargo.toml`
- Scans all `*.rs` files under `src/` for `cfg(feature = "...")` and `cfg_attr(feature = "...", ...)` references
- Exits non-zero if any referenced feature is missing from the declared set
- Uses only stdlib (`os`, `re`, `sys`) — zero external dependencies

### CI wiring

- **`check_ci.sh`**: Added `python3 scripts/check_features.py` step (picked up by `contracts-ci.yml`)
- **`.github/workflows/ci.yml`**: Added "Check feature flag consistency" step in the `build` job

### Documentation

- **`docs/ci-pipeline.md`**: Added step 5 describing the check
- **`README.md`**: Added "Feature Flag Consistency" subsection under Testing

## Backwards compatibility

Fully backwards compatible. No public API changes, no contract logic changes, no `#![no_std]` discipline violations. The new check is opt-out by virtue of being a CI gate — local development is unaffected.

## Verification

- `scripts/check_features.py` returns exit code 0 on the current codebase (the one `testutils` reference is now properly declared)
- `cargo test --all-features` continues to work — `testutils` is now a real feature that `--all-features` enables
- WASM release builds are unaffected (the event size check behind `cfg(any(test, feature = "testutils"))` is already excluded from non-test builds)
