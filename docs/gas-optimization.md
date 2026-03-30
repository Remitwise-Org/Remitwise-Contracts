# Gas Optimization Report

This document records the implemented gas/cost-efficiency pass for the RemitWise Soroban contracts and maps to the acceptance criteria described in `AGENTS.md` (without modifying `AGENTS.md` itself).

## Scope

Benchmarked/optimized contracts in this pass:

- `remittance_split`
- `savings_goals`
- `bill_payments`
- `insurance`
- `family_wallet`

`reporting` and `data_migration` were reviewed, but no gas benchmark harness exists in this repository for them yet.

## Implemented Optimizations

### Storage / Data Access

- `bill_payments`: added cached per-owner unpaid totals (`UNPD_TOT`) and maintained it on `create_bill`, `pay_bill`, `cancel_bill`, and `batch_pay_bills`
- `insurance`: added cached per-owner active premium totals (`PRM_TOT`) and maintained it on `create_policy` and `deactivate_policy`
- `savings_goals`: added owner -> goal-id index (`OWN_GOAL`) and used a hybrid read path in `get_all_goals(owner)`:
  - full-scan fast path when the owner owns all goals (avoids extra lookups)
  - index lookup path when the owner owns a subset

### Computation / Hot Paths

- `remittance_split`:
  - removed duplicate arithmetic in `calculate_split`
  - introduced internal split calculator with optional event emission
  - `distribute_usdc` now uses the non-event internal path to avoid redundant event work and temporary vector allocation
  - reduced redundant `Env`/`Address` clones in nonce helpers
- `family_wallet`:
  - removed redundant `signers.clone()` in `configure_multisig`
  - reused `signers.len()` result during threshold validation

### Code Size / Unused Code Cleanup

- Removed dead, uncompiled file `bill_payments/src/schedule.rs` (it was not linked into the crate and used `#![allow(dead_code)]`)
- Removed stale commented-out `mod schedule` scaffolding from `bill_payments/src/lib.rs`
- Verified all optimized contracts compile cleanly with `cargo check` (no compiler warnings surfaced in this pass)

## Compiler Optimizations (Verified)

The workspace root `Cargo.toml` already includes release profile settings suitable for Soroban contract size/runtime cost efficiency:

- `opt-level = "z"` (optimize for size)
- `lto = true`
- `codegen-units = 1`
- `panic = "abort"`
- `strip = "symbols"`
- `debug = 0`
- `debug-assertions = false`

These settings were retained and verified during release WASM builds.

## Gas Benchmarks (Before vs After)

Benchmark commands:

```bash
RUST_TEST_THREADS=1 cargo test -p remittance_split --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p savings_goals --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p bill_payments --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p insurance --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p family_wallet --test gas_bench -- --nocapture
```

| Contract | Method | CPU Before | CPU After | CPU Delta | Mem Before | Mem After | Mem Delta |
|---|---:|---:|---:|---:|---:|---:|---:|
| `remittance_split` | `distribute_usdc` | 656,918 | 638,399 | -18,519 (-2.82%) | 86,323 | 84,687 | -1,636 (-1.90%) |
| `savings_goals` | `get_all_goals` | 2,661,552 | 2,642,861 | -18,691 (-0.70%) | 480,721 | 495,233 | +14,512 (+3.02%) |
| `bill_payments` | `get_total_unpaid` | 1,077,221 | 1,000,587 | -76,634 (-7.11%) | 235,460 | 236,260 | +800 (+0.34%) |
| `insurance` | `get_total_monthly_premium` | 2,373,104 | 2,213,216 | -159,888 (-6.74%) | 427,575 | 428,373 | +798 (+0.19%) |
| `family_wallet` | `configure_multisig` | 307,374 | 307,374 | 0 (0.00%) | 60,934 | 60,934 | 0 (0.00%) |

Aggregate across benchmarked methods:

- CPU: `7,076,169 -> 6,802,437` (`-273,732`, `-3.87%`)
- Memory: `1,291,013 -> 1,305,487` (`+14,474`, `+1.12%`)

## Release WASM Artifact Sizes (Current)

Built with:

```bash
cargo build --release --target wasm32-unknown-unknown \
  -p remittance_split -p savings_goals -p bill_payments -p insurance -p family_wallet
```

Current artifact sizes:

- `target/wasm32-unknown-unknown/release/remittance_split.wasm`: `48,297` bytes
- `target/wasm32-unknown-unknown/release/savings_goals.wasm`: `55,527` bytes
- `target/wasm32-unknown-unknown/release/bill_payments.wasm`: `39,523` bytes
- `target/wasm32-unknown-unknown/release/insurance.wasm`: `42,057` bytes
- `target/wasm32-unknown-unknown/release/family_wallet.wasm`: `63,296` bytes

## Validation

Commands executed during this pass:

```bash
cargo check -p remittance_split -p savings_goals -p bill_payments -p insurance -p family_wallet

cargo test -p remittance_split --test gas_bench -- --nocapture
cargo test -p savings_goals --test gas_bench -- --nocapture
cargo test -p bill_payments --test gas_bench -- --nocapture
cargo test -p insurance --test gas_bench -- --nocapture
cargo test -p family_wallet --test gas_bench -- --nocapture

cargo test -p remittance_split -p savings_goals -p bill_payments -p insurance -p family_wallet
```

## Notes / Tradeoffs

- Cached aggregates improve read-path CPU for common totals but increase write work on state transitions.
- `savings_goals` indexing is deliberately hybrid to avoid regressing the single-owner-worst-case benchmark while improving subset-owner reads in multi-user scenarios.
- Snapshot fixture files under `*/test_snapshots/` changed because contract storage/state outputs changed after the new caches/indexes were introduced.
