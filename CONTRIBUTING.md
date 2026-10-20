# Contributing to Remitwise Contracts

Thank you for contributing to **Remitwise Contracts**! This document covers the required environment setup, toolchain version requirements, and the workflow for making your first contribution.

For a more in-depth orientation, see [docs/CONTRIBUTOR_OVERVIEW.md](docs/CONTRIBUTOR_OVERVIEW.md).

---

## Required Rust Toolchain

### Toolchain Version

This repository uses the **stable** Rust channel, pinned by [`rust-toolchain.toml`](rust-toolchain.toml) at the workspace root.

Current requirements (from `rust-toolchain.toml`):

| Field        | Value                                         |
|-------------|-----------------------------------------------|
| channel      | `stable`                                      |
| components   | `rustfmt`, `clippy`                           |
| targets      | `wasm32-unknown-unknown`, `wasm32v1-none`     |

> **Why stable?** Soroban's `soroban-sdk = "21.x"` and the WASM targets require a stable toolchain. Using nightly may produce incompatible artifacts or fail to link.

### Installing the Toolchain

[`rustup`](https://rustup.rs) reads `rust-toolchain.toml` automatically and will install the right toolchain on first use:

```bash
# Install rustup if you don't have it
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# From the repo root — rustup picks up rust-toolchain.toml automatically
rustup show
```

You should see output like:

```
stable-<your-host-triple> (overridden by '/path/to/Remitwise-Contracts/rust-toolchain.toml')
```

### Verifying Your Setup

Run the bundled verification script before starting any work:

```bash
bash scripts/verify_toolchain.sh
```

The script checks:

1. `rustc` is available and reports the stable channel.
2. The `wasm32-unknown-unknown` target is installed.
3. The `wasm32v1-none` target is installed.
4. `rustfmt` and `clippy` components are present.
5. A quick `cargo check` on the workspace succeeds.

Expected successful output:

```
✅ rustc stable found: rustc X.Y.Z (... stable)
✅ target wasm32-unknown-unknown installed
✅ target wasm32v1-none installed
✅ component rustfmt installed
✅ component clippy installed
✅ cargo check passed
✅ All toolchain checks passed.
```

If any check fails, the script exits with a non-zero status and prints a remediation hint.

---

## Development Prerequisites

| Tool              | Version       | Install                                        |
|-------------------|---------------|------------------------------------------------|
| Rust (stable)     | see above     | `rustup toolchain install stable`              |
| Soroban CLI       | `21.0.0`      | `cargo install --locked --version 21.0.0 soroban-cli` |
| Python 3          | `3.9+`        | Used by workspace invariant scripts             |

---

## Pre-PR Checklist

Run all steps locally before opening a pull request. CI enforces the same checks.

```bash
# 1. Verify your toolchain
bash scripts/verify_toolchain.sh

# 2. Build all WASM contracts
cargo build --release --target wasm32-unknown-unknown

# 3. Run tests for the affected crate(s)
cargo test -p orchestrator
cargo test -p remitwise-common

# 4. Lint (no warnings, no unwrap/expect in production code)
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --lib -- -D clippy::unwrap_used -D clippy::expect_used

# 5. Format check
cargo fmt --all -- --check
```

Or run the single CI gate script that chains all of the above:

```bash
bash check_ci.sh
```

---

## Continuous Integration (CI)

Our CI workflow (`.github/workflows/ci.yml`) enforces automated quality gates on every push and pull request to `main`:

### Gated Crates (Blocking)
Currently, three crates are part of the blocking CI gate matrix (tested across both MSRV `1.88.0` and `stable`):
- `remitwise-common` (library and tests)
- `orchestrator` (contract, integration tests, and release WASM build)
- `reporting` (contract, integration tests, and release WASM build)

Each gated crate must pass:
1. `cargo fmt -p <crate> -- --check`
2. `cargo clippy -p <crate> --all-targets -- -D warnings`
3. `cargo test -p <crate>`
4. Ignored test budget verification (cannot exceed the current count recorded in `KNOWN_FAILING_TESTS.md`: max 4 for `remitwise-common`, max 8 for `orchestrator`, max 1 for `reporting`)
5. `cargo build -p <crate> --target wasm32-unknown-unknown --release` (for contract crates)

### Non-Gated Crates & Promotion
Other crates (`savings_goals`, `insurance`, `bill_payments`, `remittance_split`, `family_wallet`, `emergency_killswitch`, `data_migration`) are monitored via the non-blocking `broken-crates` CI job, which reports a pass/fail status table to the GitHub Actions step summary.

**How to promote a crate to the blocking CI gate:**
1. Fix compilation, clippy warnings, formatting, and tests for the target crate.
2. In `.github/workflows/ci.yml`, add the crate name to the `matrix.crate` array in the `contracts` job.
3. Remove the crate from the `broken-crates` check list.
4. If the crate was excluded from the workspace (in root `Cargo.toml`), move it into `workspace.members` if ready.
5. Submit a PR and verify the blocking gate runs and passes.

---

## Repo-Specific Rules

- **No `#[allow(unused)]` in production code** without a comment explaining why.
- **No hard-coded limits in call sites** — put them in `params.rs` (see `remittance_split/src/params.rs` for the pattern).
- **Prefer typed newtypes** (`Amount`, `Percent`, etc.) over raw `i128` — do not lose the currency tag.
- **`#![no_std]` discipline** in every contract crate — use `soroban_sdk` types, not `std::vec::Vec`.
- **`require_auth()` first** in every state-mutating entrypoint.
- Follow `Closes #<issue>` in PR descriptions.

---

## How to Claim an Issue

1. Comment on the issue that you'd like to take it on and wait for a maintainer to assign it to you. This avoids duplicated effort.
2. Open a PR that references the issue (`Closes #NNNN`).
3. Make sure CI is green and request a review from a `CODEOWNERS` maintainer.

---

## Further Reading

- [Architecture Overview](ARCHITECTURE.md)
- [Storage Layout Reference](STORAGE_LAYOUT.md)
- [Authorization Matrix](docs/AUTHORIZATION_MATRIX.md)
- [Threat Model](THREAT_MODEL.md)
- [Changelog](CHANGELOG.md)
