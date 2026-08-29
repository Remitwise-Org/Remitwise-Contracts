# Atomic Rollback Design — Bill Payments

## Overview

This document describes the atomic rollback pattern implemented for the
`bill_payments` contract to satisfy issue #1739.

## Problem

The original implementation modified contract state **during iteration** over
bill records. While Soroban transactions are ledger-atomic (a failed `Err`
reverts all storage writes), the original code had two structural weaknesses:

1. **Compound operations mixed computation with mutation.** `pay_bill` set
   `bill.paid = true` on the in-memory copy *before* computing the recurring
   child bill. If the child computation overflowed, Soroban would revert the
   entire transaction — but the code's intent was unclear and error-prone.

2. **Batch operations emitted events during mutation.** `batch_pay_bills` and
   `archive_paid_bills` called `env.events().publish()` inside the loop that
   also mutated storage, making it impossible to reason about atomicity without
   tracing every code path.

## Solution: Validate-Then-Commit (Two-Phase)

All mutating operations now follow a strict two-phase pattern:

### Phase 1: Compute (read-only)

- Load current state from storage
- Validate all preconditions
- Compute all side-effects (child bills, archived bill structs, etc.)
- Store computed results in staging buffers (`Map<u32, Bill>`, etc.)
- **No storage writes, no event emissions** during this phase

### Phase 2: Commit (write-only)

- Apply all computed mutations to storage
- Emit all events
- Update indexes
- Adjust aggregate counters

**Invariant:** If Phase 1 fails at any point (overflow, not-found, unauthorized),
zero storage has been modified and zero events have been emitted. The transaction
reverts cleanly.

## Changed Functions

### `pay_bill` (existing, unchanged signature)

Restructured to compute child bill (recurring) before modifying the parent bill.
The child's `due_date` overflow check now happens **before** any state mutation.

### `pay_bill_atomic` (new)

Returns `AtomicPayReceipt` instead of `()`. Same logic as `pay_bill` but exposes
the deterministic outcome for verification at the integration boundary:

```rust
pub struct AtomicPayReceipt {
    pub bill_id: u32,
    pub paid_amount: i128,
    pub child_bill_id: Option<u32>,
    pub child_due_date: Option<u64>,
}
```

### `batch_pay_bills` (existing, unchanged signature)

Restructured to:
1. **Phase 1:** Scan all bill IDs, validate ownership/paid status, compute child
   bills into `staging_child: Map<u32, Bill>`, and mark paid bills in
   `staging_paid: Map<u32, Bill>`. Parent→child linkage tracked via
   `parent_to_child: Map<u32, u32>`.
2. **Phase 2:** Commit all mutations at once — set bills, update NEXT_ID,
   update indexes, emit events.

If any recurring computation overflows during Phase 1, the entire batch reverts
with zero state changes.

### `archive_paid_bills` (existing, unchanged signature)

Restructured to:
1. **Phase 1:** Scan qualifying bills into `staging_archived: Map<u32,
   ArchivedBill>` and track owner/currency index changes.
2. **Phase 2:** Release external refs, commit archived bills, update indexes.

## New Error Variants

```rust
pub enum BillPaymentsError {
    // ... existing variants ...
    AtomicRollbackFailed = 20,  // Reserved for future rollback compensation
    ScheduleOverflow = 21,      // Scheduling arithmetic overflow
}
```

## New Types

- `AtomicPayReceipt` — receipt from `pay_bill_atomic`
- `AtomicBatchPayReceipt` — receipt from batch operations (reserved for future use)

## Failure Behavior

| Scenario | Before | After |
|----------|--------|-------|
| `pay_bill` recurring overflow | Soroban reverts all writes | Same — but code structure makes this explicit |
| `batch_pay_bills` mixed valid/invalid | Invalid skipped, valid processed | Same — but computation is separated from mutation |
| `batch_pay_bills` recurring overflow mid-batch | Soroban reverts all writes | Same — but Phase 1 never writes, so failure is cleaner |
| `archive_paid_bills` partial qualification | External refs released during scan | External refs released only in Phase 2 |
| Repeated `pay_bill` on same bill | Returns `BillAlreadyPaid` | Same — verified by test |

## Migration / Rollback

- No storage layout changes
- No public API signature changes (new `pay_bill_atomic` is additive)
- No migration required
- Can be rolled back by reverting the commit

## Security Assumptions

- Soroban transaction atomicity guarantees: if any `?` operator propagates
  an error, ALL storage writes in that transaction are reverted at the ledger
- Events emitted during a reverted transaction are not persisted
- The two-phase pattern does NOT introduce new trust assumptions — it only
  makes the existing atomicity explicit in the code structure

## Test Coverage

| Test | What it validates |
|------|-------------------|
| `test_pay_bill_atomic_non_recurring_receipt` | Receipt correctness for one-shot bills |
| `test_pay_bill_atomic_recurring_receipt` | Receipt correctness with child bill info |
| `test_pay_bill_atomic_already_paid_no_partial_state` | Failed atomic pay leaves no partial state |
| `test_batch_pay_bills_mixed_valid_invalid` | Mixed valid/invalid IDs processed correctly |
| `test_batch_pay_bills_atomic_rollback_on_overflow` | Batch overflow reverts entire batch |
| `test_archive_paid_bills_atomic_no_qualifying` | No-op when no bills qualify |
| `test_archive_paid_bills_atomic_all_qualifying` | All qualifying bills archived atomically |
| `test_repeated_pay_bill_no_partial_state` | Repeated pay leaves no corruption |
| `test_pay_bill_atomic_matches_pay_bill` | Atomic and non-atomic produce identical state |
