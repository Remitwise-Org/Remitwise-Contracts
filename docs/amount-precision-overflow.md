# Amount Precision and Overflow Validation

## Overview

This document describes the amount precision and overflow validation system implemented
in `data_migration/src/lib.rs` as part of Issue #1767. The system provides deterministic,
reviewable guarantees that payment and settlement data maintains correct precision and
does not introduce overflow at the import boundary.

## Motivation

The current behavior or coverage needs a production-grade guarantee: lost records or
cursor gaps make financial reconciliation impossible after upgrades. Without this work,
rounding, truncation, or overflow changing balances and user-visible totals could survive
review or deployment.

## Design

### Constants

| Constant | Value | Purpose |
|---|---|---|
| `MAX_AMOUNT` | `i64::MAX` | Maximum allowed monetary amount (inclusive) |
| `MIN_AMOUNT` | `0` | Minimum allowed monetary amount (non-negative) |
| `NEAR_OVERFLOW_THRESHOLD` | `i64::MAX / 4` (~2.3 × 10¹⁸) | Defence-in-depth threshold for near-overflow rejection |
| `MAX_AMOUNT_SCALE` | `7` | Maximum decimal digits (matches Stellar stroop precision) |

### Validation Function

```rust
pub fn validate_amount(field: &'static str, value: i64) -> Result<(), MigrationError>
```

Checks performed (in order):
1. **Non-negative** — `value >= MIN_AMOUNT` (0)
2. **Within bounds** — `value <= MAX_AMOUNT` (i64::MAX)
3. **Below near-overflow threshold** — `value <= NEAR_OVERFLOW_THRESHOLD`

### Error Variants

| Variant | Trigger | Example |
|---|---|---|
| `NegativeAmount` | `value < 0` | `-1`, `i64::MIN` |
| `AmountOverflow` | `value > i64::MAX` | `i64::MAX + 1` (unreachable for i64, defensive) |
| `AmountNearOverflow` | `value > NEAR_OVERFLOW_THRESHOLD` | `NEAR_OVERFLOW_THRESHOLD + 1`, `i64::MAX` |
| `InvalidAmountScale` | Too many decimal digits | Reserved for future use |

### Integration Points

The validation is enforced at every import boundary:

1. **`validate_payload_semantics`** — Called during `validate_for_import()` for all
   snapshot types. Validates every `target_amount` and `current_amount` in
   `SavingsGoalExport` entries.

2. **CSV import** (`import_goals_from_csv`) — Uses `validate_amount` instead of
   ad-hoc negative checks for consistent validation across all import paths.

3. **JSON import** (`import_from_json` / `import_from_json_untracked`) — Delegates
   to `validate_for_import` → `validate_payload_semantics`.

4. **Binary import** (`import_from_binary` / `import_from_binary_untracked`) — Same
   delegation path.

### Near-Overflow Threshold Rationale

The threshold `i64::MAX / 4` is chosen to leave ample headroom for:

- **Addition**: Two amounts at threshold sum to `i64::MAX / 2`, safely below overflow.
- **Subtraction**: `threshold - threshold = 0`, no underflow.
- **Basis-point multiplication**: `threshold * 10_000 / 10_000 = threshold`, no overflow
  for standard financial scale factors.

This is a defence-in-depth guard: even if downstream contracts perform arithmetic on
imported amounts, the values are safe by construction.

## Invariants

### At Import Boundary

1. All monetary amounts are non-negative.
2. All monetary amounts are below the near-overflow threshold.
3. No amount exceeds i64::MAX.
4. Rejected operations leave no partial or unauthorized state.

### Reconciliation

- Gap-free enumeration via `MigrationTracker::imported_records` is unaffected.
- Deterministic order (BTreeMap storage) is preserved.
- Amount validation errors are terminal: the snapshot is rejected and the tracker
  is not mutated.

## Failure Behavior

| Scenario | Behavior |
|---|---|
| Negative amount in any field | `NegativeAmount` error, tracker unchanged |
| Amount at NEAR_OVERFLOW_THRESHOLD | Accepted (boundary is inclusive) |
| Amount at NEAR_OVERFLOW_THRESHOLD + 1 | `AmountNearOverflow` error, tracker unchanged |
| Amount at i64::MAX | `AmountNearOverflow` error, tracker unchanged |
| Repeated rejections of same payload | Each rejection leaves no state, idempotent |
| Rejection after a valid import | Valid import marker preserved, no side effects |

## Compatibility Impact

- **Backward compatible**: Existing valid snapshots are unaffected. The new validation
  only rejects values that would cause overflow in downstream arithmetic.
- **Migration required**: No on-chain state changes. Validation is import-time only.
- **Rollback**: No rollback needed. The new checks are additive guards.

## Security Assumptions

- The `NEAR_OVERFLOW_THRESHOLD` provides sufficient headroom for standard financial
  arithmetic (addition, subtraction, basis-point multiplication).
- Downstream contracts may perform arithmetic on imported amounts; the threshold
  ensures this arithmetic cannot overflow.
- The validation is fail-closed: any invalid amount causes the entire import to fail,
  preventing partial state application.

## Validation Commands

```bash
# Run all tests in the data_migration crate
cargo test --package data_migration

# Run only the amount precision tests
cargo test --package data_migration -- amount

# Run the proptest suite
cargo test --package data_migration -- proptest

# Run clippy for lint checks
cargo clippy --package data_migration -- -D warnings

# Run format check
cargo fmt --package data_migration -- --check
```
