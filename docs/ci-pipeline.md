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
5. **Feature Flag Consistency**: Runs `scripts/check_features.py` to verify that every `cfg(feature = "...")` reference in Rust source corresponds to a declared entry in the crate's `[features]` section. This prevents dead or silently ignored feature gates.
6. **Build Verification**:
    - Contracts are compiled using `--target wasm32-unknown-unknown --release`.
    - The CLI is compiled natively.

## WASM Size Diff (`wasm-size.yml`)

Every pull request targeting `main` triggers the **WASM Size Diff** workflow (`.github/workflows/wasm-size.yml`). It builds each Soroban contract to `wasm32-unknown-unknown` twice — once on the PR HEAD and once on the merge base — then posts (or updates) a single PR comment showing the binary-size delta per entrypoint.

### What it reports

| Column | Meaning |
| ------ | ------- |
| **Before** | WASM byte size on the base branch |
| **After** | WASM byte size on the PR HEAD |
| **Δ bytes** | Signed difference (negative = shrink) |
| **Δ %** | Relative change |

Icon key: ✅ no significant change · ✨ shrank > 512 B · ⚠️ grew > 1 kB · 🆕 new contract · 🗑️ contract removed.

### Local equivalent

```bash
# Build all WASM contracts, then collect sizes:
for contract in remittance_split savings_goals bill_payments insurance \
                family_wallet orchestrator data_migration emergency_killswitch reporting; do
  cargo build --release --target wasm32-unknown-unknown --package "$contract"
done
./scripts/collect_wasm_sizes.sh
```

`scripts/collect_wasm_sizes.sh` prints a JSON object of `{ "contract_name": byte_size, … }` to stdout, or to a file path passed as its first argument.

### Notes

- Sizes are **pre-optimizer** release WASM. On-chain deployment uses the Soroban optimizer, so absolute numbers differ from deployed contract sizes — but the delta between two commits on the same toolchain is accurate.
- A contract that fails to build is reported as size `0`. The workflow does not fail the PR on build errors in this job; use the main `ci.yml` build gate for that.
- The comment is updated in-place on each push to the PR (identified by a hidden HTML marker), so the thread stays clean.

## Soroban SDK Updates

This pipeline acts as a regression gate for SDK upgrades. When bumping the SDK version (e.g., to `21.7.7` and beyond), the CI must pass before merging.

For the complete validation process during a Soroban SDK upgrade, refer to the [Soroban Version Checklist](../.github/SOROBAN_VERSION_CHECKLIST.md).