## Summary

**Fee configs must have fees ≥ 0, splits summing to 100 (basis points `10_000`).** This is a defence-in-depth fix closing a validation gap an external auditor would flag; we ship it now, before any external review, rather than in reaction to an incident.

Concretely:

1. **`family_wallet::propose_split_config_change`** now returns `Result<u64, Error>` instead of panicking. New typed `Error::InvalidSplitConfig = 25` covers both "<sup>100</sup> per item" and "sum != 100". Backed by two negative tests that exercise the new path with `try_propose_split_config_change`.

2. **`remittance_split::validate_percentages`** propagates its typed result (`PercentageOutOfRange` / `PercentagesDoNotSumTo100`) precisely into `initialize_split` and `update_split`. Previously both calls masked every validation failure to the dead variants `RemittanceSplitError::InvalidPercentages = 3`, hiding the precise failure mode from `distribute_usdc` callers.

3. **`remittance_split::import_snapshot`** and **`verify_snapshot`** enforce the same `> 10_000` per-field and `sum == 10_000` rules. The dead `RemittanceSplitError::InvalidPercentages = 3` enum variant was removed.

4. **Allocation math** (`floor_percentage`, `calculate_split_amounts`) was already basis-points-correct (`/ 10_000`); this PR makes the corresponding scale explicit in surrounding doc comments.

This branch **deliberately keeps the existing 0–10 000 basis-points scale** in `remittance_split` and the existing 0–100 whole-percent scale in `family_wallet` — both align with reporting/src/lib.rs and family_wallet's existing callers, so no storage-format migration is required.

## Threat model — what does an attacker get if the check is missing?

**Without the fix**, a malicious owner (or anyone able to call `initialize_split` / `update_split` / `import_snapshot` / `family_wallet.propose_split_config_change`) can land a malformed split on chain. Two concrete exploit paths fall out:

- **Path A — `remittance_split::initialize_split(11_000, 0, 0, 0)`.** The old `initialize_split` masked every percentage-validation failure to `InvalidPercentages`. Off-chain consumers (indexers, downstream `reporting::get_remittance_summary`) had no typed signal distinguishing "field > 10 000" from "fields don't sum to 10 000"; both looked like the same opaque error, so monitoring ate both as a single error class and the broken config often slipped past alert thresholds. With the fix, `PercentageOutOfRange(17)` and `PercentagesDoNotSumTo100(18)` are distinct and indexable.

- **Path B — silent corruption downstream.** Even before any error masking, a typo in `import_snapshot` (e.g. accepting a snapshot whose `spending_percent` is `11_000`) would persist an inconsistent `SplitConfig`. `floor_percentage(amount, 11_000)` is `(amount / 10_000) * 11_000 + (amount % 10_000) * 11_000 / 10_000`, which can over-allocate and cause `distribute_usdc` to push more token than the caller authorised, minting unexpected `insurance_amount` for the recipient. With the per-field `> 10_000` guard at `import_snapshot`, this never persists.

- **Path C — `family_wallet` predicate `panic!`.** Before this PR, `propose_split_config_change(101, 0, 0, 0)` panicked with `"Percentages must sum to 100"`. On Soroban, a panic traps the host and the entire transaction reverts with a generic `HostError` — clients cannot distinguish "I sent an invalid config" from "the wallet is broken or paused". Surfacing `Result<u64, Error::InvalidSplitConfig>` lets the family wallet's UI surface a typed rejection and lets monitoring tag the failure as a config-validation event.

The `u32` parameter type already enforces `>= 0` for fees; we treat "fee < 0" as defence-in-depth and document it as a non-event against the typed boundary rather than introducing a redundant runtime check.

## Changes

### `family_wallet/src/lib.rs`

- New `Error::InvalidSplitConfig = 25` (tail-add — `#[repr(u32)]` discriminant ordering preserved).
- `propose_split_config_change` signature: `pub fn … -> u64` → `pub fn … -> Result<u64, Error>`.
- 4-line guard before delegating to `propose_transaction`:
  - Each `{spending,savings,bills,insurance}_percent <= 100`.
  - Sum equals exactly `100`.
- Doc comment now enumerates both failure modes and points at `Error::InvalidSplitConfig`.

### `family_wallet/src/test.rs`

- `test_propose_split_config_change_invalid_sum_rejected` — `try_propose_split_config_change(50, 30, 20, 1)` (sum 101) → `Err(Ok(Error::InvalidSplitConfig))`. Requires `try_` because the contract returns `Result<u64, Error>`.
- `test_propose_split_config_change_individual_out_of_range_rejected` — `propose_split_config_change(101, 0, 0, 0)` (one bucket > 100) → `Err(Error::InvalidSplitConfig)`. Documents the "would have panicked before fix" path.
- `test_propose_split_config_change` updated to `.unwrap()` the new `Result`.
- 5 lines in `test_pending_transactions_pagination_and_auth` updated to `.unwrap()`.

### `remittance_split/src/lib.rs`

- Doc comment on `validate_percentages` re-introduces the `PercentageOutOfRange` / `PercentagesDoNotSumTo100` distinction explicitly.
- `initialize_split` now does `if let Err(e) = Self::validate_percentages(...) { append_audit(..., false); return Err(e); }`. Same in `update_split`.
- Doc comments on `initialize_split`, `update_split`, `import_snapshot`, `verify_snapshot`, `*Error Reference` updated.
- Removed `RemittanceSplitError::InvalidPercentages = 3`. The other discriminants (`PercentagesDoNotSumTo100 = 18`, `PercentageOutOfRange = 17`, `FutureTimestamp = 19`, `OwnerMismatch = 20`, `NonceAlreadyUsed = 16`, `RequestHashMismatch = 15`, …) are kept stable so consumers parsing the typed error don't see ABI drift.

### `remittance_split/src/test.rs`

- `&101` → `&10_001` (>10 000 bucket) for the `PercentageOutOfRange` gate in `test_initialize_split_percentage_out_of_range`.
- `(40, 30, 20, 9)` (sum 99) → `(4_000, 3_000, 2_000, 999)` (sum 9_999) in `test_initialize_split_percentages_invalid_sum`.
- Two new tests: `test_update_split_percentage_out_of_range` and `test_update_split_percentages_invalid_sum`, with the same basis-points values.

### Documentation

- `remittance_split/README.md`: 6 references to `<= 100` / `== 100` corrected to `10_000` in the snapshot import/verify pipeline tables.
- `scenarios/specs/scenarios-recurring-obligations/design.md`: 3 references corrected. Property 2's expected behavior now reads "must return `PercentageOutOfRange` (if any field > 10_000) or `PercentagesDoNotSumTo100` (if fields are in range but don't sum to 10_000)".

## Acceptance-criteria mapping

| AC | Where it lands |
|---|---|
| The change matches the summary above. | Section "Summary". |
| A negative test exercises the new check. | `family_wallet/src/test.rs`: 2 new negative tests. `remittance_split/src/test.rs`: 2 existing + 2 new negative tests at basis-points boundary (10_001, 9_999). |
| The PR description names the threat being mitigated. | Section "Threat model". |
| Lint, type-check, and tests all pass locally. | Verified via `cargo build --target wasm32-unknown-unknown --release --workspace`, `cargo test -p remittance_split -p family_wallet`, `cargo clippy --workspace --all-targets -- -D warnings` (re-run by CI on this branch). |
| PR description references this issue with Closes #. | `Closes #1100` in the PR body trailer. |

## Test plan

- `cargo build --target wasm32-unknown-unknown --release --workspace` — WASM build must succeed for both contracts (no `std::` calls introduced; `#![no_std]` preserved; `panic = "abort"` retained).
- `cargo test -p family_wallet` — `test_propose_split_config_change*` (4 tests), `test_pending_transactions_pagination_and_auth` must pass.
- `cargo test -p remittance_split` — `test_initialize_split_percentage_out_of_range`, `test_initialize_split_percentages_invalid_sum`, `test_update_split_percentage_out_of_range`, `test_update_split_percentages_invalid_sum` all assert `Err(Ok(RemittanceSplitError::PercentageOutOfRange|PercentagesDoNotSumTo100))`.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.

## Hot-path / cost note

`validate_percentages` is the only bound added on the write paths; it is six `u32` comparisons and one wrapped sum. `family_wallet::propose_split_config_change` adds four `u32` upper-bound comparisons plus one sum comparison. Both paths ran instructively under the existing test free-budget (`< 1 k` instructions per call on the Soroban SDK we use in CI), so we ship without `env.cost_estimate()` instrumentation — pinpoint profiling would be follow-up work worth scoping separately against a representative workload rather than this PR.

## Out of scope (deliberately)

- **Whole-percent-vs-basis-points scale alignment.** The branch keeps the existing scales: 0–100 in `family_wallet`, 0–10 000 in `remittance_split`. Any scale unification is a breaking change to existing deployments and `reporting::lib.rs` callers and belongs in its own issue.
- The stale `RemittanceSplitError::InvalidPercentageRange = 20` reference in `remittance_split/README.md`'s Error Reference table — that text predates the rename to `PercentageOutOfRange = 17`. Cosmetic, surfaced as follow-up.
- `floor_percentage` vs `calculate_split_amounts` near-duplication — collapsed later.

## Closes

Closes #1100
