# Cross-Contract Invariant Tests

## Scope

This issue adds cross-contract invariant coverage for remittance allocation consistency.

Covered invariants:

- `calculate_split` remains lossless after integer division.
- Savings allocations are recorded in `savings_goals` without category drift.
- Bill allocations are recorded in `bill_payments` without category drift.
- Insurance allocations are recorded in `insurance` without category drift.
- Aggregate downstream totals remain equal to `total_remittance - spending`.
- Paying a bill reduces unpaid bill totals without mutating savings balances.
- Deactivating a policy reduces active premium totals without mutating savings balances.

## Files

- `integration_tests/tests/multi_contract_integration.rs`
- `scripts/verify_cross_contract_invariants.py`
- `scripts/README_INVARIANT_TESTS.md`

Prerequisite compile-repair files also touched so the branch can build:

- `bill_payments/src/lib.rs`
- `insurance/src/lib.rs`
- `savings_goals/src/lib.rs`

## Run

```bash
python3 scripts/verify_cross_contract_invariants.py
cargo test -p integration_tests
```

## Expected Output

The verifier prints four cases:

- single remittance
- cumulative remittances
- rounding remainder
- downstream state transitions

It ends with:

```text
all cross-contract invariant checks passed
```

The integration crate includes four new invariant-focused tests on top of the
existing suite. The important signal is that the new tests pass alongside the
existing integration coverage.

## Security Notes

- The verifier assumes all downstream writes are intentional applications of the split output.
- The tests focus on accounting invariants, not token transfer authorization.
- Bills are created as non-recurring in the invariant suite so unpaid totals stay directly traceable to the recorded allocation.
- Insurance totals use active monthly premium totals, so deactivation must reduce the aggregate immediately.
