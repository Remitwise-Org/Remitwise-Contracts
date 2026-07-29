# CI Failure Diagnosis and Fix

## Problem

The PR introduced a new feature-consistency check that is now executed by both CI entry points:

- `.github/workflows/ci.yml` adds `python3 scripts/check_features.py`
- `check_ci.sh` adds `python3 scripts/check_features.py`

As a result, the following two jobs fail:

- `Build and Test` (main CI workflow)
- `Validate and Build` (Batch-B CI gate via `check_ci.sh`)

The failure appears to be caused by the new feature-check integration itself, not by the existing Rust build or contract logic. GitHub API annotations show the job exits with code `1`, and the only new runnable command in the failing workflow paths is the Python feature-check script.

## Likely root cause

- The new CI step runs `python3 scripts/check_features.py`
- The CI environment may not have `python3` available under that command
- Or `scripts/check_features.py` may be exiting with a non-zero status due to an unexpected parse or workspace detection issue

## Fix

1. Update CI wiring to verify Python availability before running the script:

   - In `.github/workflows/ci.yml`
   - In `check_ci.sh`

   Use a fallback such as:

   ```bash
   if command -v python3 >/dev/null 2>&1; then
     python3 scripts/check_features.py
   elif command -v python >/dev/null 2>&1; then
     python scripts/check_features.py
   else
     echo "Error: Python is not installed"
     exit 1
   fi
   ```

2. Add diagnostics to `scripts/check_features.py` so failures are visible:

   - print the workspace root
   - print discovered workspace members
   - print declared features and referenced features for each crate
   - propagate parse exceptions clearly

3. Re-run the workflows after the CI fix to confirm the failure is isolated to the feature-check step.

4. If the script still fails, inspect the output for the exact missing or malformed feature declarations and fix the corresponding `Cargo.toml` entries.

## Goal

Ensure that the feature flag consistency check is robust in CI and that the new `python3` invocation does not cause spurious failures.
