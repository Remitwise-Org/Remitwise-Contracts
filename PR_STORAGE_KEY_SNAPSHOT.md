## Summary

Snapshots the keys + types and fails if a future change reuses an old key.

Adds a deterministic, self-validating test that locks in the current
(contract, key, type, storage-tier) mapping for every storage key across
all 8 contracts. The test catches schema-evolution regressions at CI time.

## Changes

### `testutils/tests/storage_key_snapshot_test.rs` (new)

114 storage-key entries documented across:

| Contract              | Entries |
|-----------------------|---------|
| remittance_split      | 11      |
| savings_goals         | 17      |
| bill_payments         | 18      |
| insurance             |  7      |
| family_wallet         | 27      |
| reporting             |  7      |
| orchestrator          | 14      |
| emergency_killswitch  |  5      |

Two enforcement tests:

1. **`test_storage_key_type_snapshot_unchanged`** — validates every entry
   has a non-empty key, type name, and tier; rejects duplicates and
   type/tier conflicts within the snapshot.

2. **`test_no_key_reused_with_different_type`** — maintains a
   `HistoricallyUsedKeys` set that never shrinks. If a retired key is
   ever re-added with a different Rust type or a different storage tier,
   the test fails.

### `remitwise-common/Cargo.toml` (incidental)

Merged three duplicate `[features]` sections into one to fix a
`duplicate key` manifest error that blocked `cargo test`.

## Testing

- `cargo check --package testutils` — passes
- `cargo clippy --package testutils -- -D warnings` — clean
- `cargo test --package testutils` — runs under CI (macOS/Ubuntu);
  blocked on this dev machine only by a missing `dlltool.exe`
  (MinGW toolchain dependency of `backtrace` on Windows GNU)
- WASM build (`--target wasm32-unknown-unknown`) — pre-existing
  `remitwise-common` errors unrelated to this change

## Closes

Closes #882
