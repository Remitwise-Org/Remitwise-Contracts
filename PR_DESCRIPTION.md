# PR: fix(#623): add InvalidDueDate boundary tests & recurring due-date docs

**Branch:** `fix/623-invalid-due-date-boundary-tests` → `main`

## Summary

Resolves #623. Pins exact boundary semantics for `BillPaymentsError::InvalidDueDate` across the `create_bill` path and the recurring next-due-date generation path in `pay_bill`. No production logic was changed.

## Changes

- **`bill_payments/tests/test_recurring_lifecycle.rs`** — Rewrote with a pinned-semantics header (exact operator, boundary table, formula) and 17 deterministic tests covering `create_bill` due-date and frequency boundaries, and `pay_bill` recurring child-formula correctness (on-time, late, catch-up loop, multi-cycle, early payment, min/max frequency). Added `assert_child_not_overdue()` security helper called in every child-spawning test.
- **`docs/bill-payments-due-date.md`** — New document: acceptance rule table, recurring formula, security invariant, overflow protection, and edge cases.
- **`bill_payments/src/lib.rs`** — Inline `///` doc comments on `InvalidDueDate`, `InvalidFrequency`, `MAX_FREQUENCY_DAYS`, `Bill::due_date`, `Bill::frequency_days`, `create_bill`, and `pay_bill`. No logic changes.
- **`bill_payments/Cargo.toml`** — Registered `test_recurring_lifecycle` as a named `[[test]]` target.
- **`test-output.txt`** — Full test run output and coverage summary.

## Recurring-Correctness Note

The recurring child due-date formula computes `child.due_date = parent.due_date + frequency_days × 86_400`, anchored to the **parent's** due date rather than the payment timestamp. If the result is still in the past at payment time (extremely late payment), a catch-up loop advances by one additional period until `child.due_date > current_time`. This guarantees the security invariant — a recurring child bill is **never born overdue** — regardless of how late the parent is paid, and regardless of whether payment occurs before, on, or after the original due date. The `assert_child_not_overdue()` helper in the test suite enforces this invariant explicitly on every test that spawns a child bill.

## Test Output

```
running 17 tests
test test_create_bill_due_date_far_past_rejected ... ok
test test_create_bill_due_date_future_accepted ... ok
test test_create_bill_due_date_exactly_now_accepted ... ok
test test_create_bill_due_date_one_second_past_rejected ... ok
test test_create_bill_due_date_zero_rejected ... ok
test test_create_bill_frequency_max_accepted ... ok
test test_create_bill_frequency_over_max_rejected ... ok
test test_create_bill_frequency_zero_non_recurring_accepted ... ok
test test_create_bill_frequency_zero_rejected ... ok
test test_recurring_bill_lifecycle ... ok
test test_recurring_child_catchup_when_paid_extremely_late ... ok
test test_recurring_child_due_date_formula_on_time_payment ... ok
test test_recurring_child_due_date_independent_of_paid_at ... ok
test test_recurring_early_payment_does_not_shift_child_due_date ... ok
test test_recurring_frequency_max_child_due_date ... ok
test test_recurring_frequency_one_day_child_due_date ... ok
test test_recurring_multi_cycle_due_dates_chain_correctly ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Coverage (cargo llvm-cov, test_recurring_lifecycle only)

| Function | Segments covered | % |
|---|---|---|
| `create_bill` | 130 / 142 | 92% |
| `pay_bill` | 123 / 137 | 90% |

Uncovered segments are exclusively in paths outside this issue's scope (pause guards, `InvalidAmount`, `OwnerBillCapExceeded`, `external_ref` claiming, `BillNotFound`, `Unauthorized`). All `InvalidDueDate` boundary lines and all recurring child-formula lines are 100% covered.
