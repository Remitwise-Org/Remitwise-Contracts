# Continuous Integration Pipeline

The RemitWise CI pipeline ensures code quality, formatting consistency, and build integrity for all pull requests.

## Workflow Overview

The primary CI gate is driven by `.github/workflows/contracts-ci.yml`, which serves as an automated wrapper around the local `check_ci.sh` script logic.

The **Batch-B CI Gate** targets the following crates:
- `family_wallet`
- `orchestrator`
- `data_migration`
- `emergency_killswitch`
- `cli`

### Pipeline Steps
1. **Toolchain Setup**: Pins the Rust compiler version via `rust-toolchain.toml` and installs the `wasm32-unknown-unknown` target.
2. **Formatting**: Runs `cargo fmt --check` to enforce stylistic consistency.
3. **Linting**: Runs `cargo clippy -D warnings` to fail fast on any anti-patterns or code smells.
4. **Testing**: Runs `cargo test` to execute the full unit and integration test harness (requiring a minimum 95% test coverage).
5. **Build Verification**:
    - Contracts are compiled using `--target wasm32-unknown-unknown --release`.
    - The CLI is compiled natively.

## Soroban SDK Updates

This pipeline acts as a regression gate for SDK upgrades. When bumping the SDK version (e.g., to `21.7.7` and beyond), the CI must pass before merging.

For the complete validation process during a Soroban SDK upgrade, refer to the [Soroban Version Checklist](../.github/SOROBAN_VERSION_CHECKLIST.md).