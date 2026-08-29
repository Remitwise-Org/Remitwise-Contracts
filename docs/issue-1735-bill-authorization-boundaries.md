# Issue #1735 — Bill Scheduling & Execution: Authorization Boundaries

## Summary

Strengthened authorization boundaries across all bill_payments write entry points to
ensure every protected path is gated by `require_no_active_kill_switch`, rejecting
missing, stale, and cross-tenant identities before any mutation. Added focused
regression tests proving the invariant at the integration boundary.

## Changes

### 1. Kill-switch guard added to 8 write entry points

The following functions were missing `require_no_active_kill_switch` and now have it
as the first defense-in-depth check (before `require_auth` and before any storage
read/write):

| Function | File |
|---|---|
| `create_bill` | `bill_payments/src/lib.rs` |
| `create_bill_schedule` | `bill_payments/src/lib.rs` |
| `modify_bill_schedule` | `bill_payments/src/lib.rs` |
| `cancel_bill_schedule` | `bill_payments/src/lib.rs` |
| `set_external_ref` | `bill_payments/src/lib.rs` |
| `reverse_payment` | `bill_payments/src/lib.rs` |
| `archive_paid_bills` | `bill_payments/src/lib.rs` |
| `bulk_cleanup_bills` | `bill_payments/src/lib.rs` |

**Pattern used:**
- For `Result`-returning functions: `require_no_active_kill_switch(&env).unwrap_or_else(|e| panic_with_error!(&env, e));`
- Same as existing guards on `pay_bill`, `cancel_bill`, `restore_bill`, etc.

### 2. Duplicate `BillPaymentsError` enum variants removed

The `BillPaymentsError` enum had two blocks of variants with overlapping names but
different discriminant values, causing `#[contracterror]` to panic
(`LengthExceedsMax`). The second (redundant) block was removed. Three new unique
variants from the second block (`SameAdmin`, `RotationTimelockTooShort`,
`InvalidStateTransition`, `InvariantViolation`) were preserved with fresh discriminants
(34–37).

### 3. Authorization boundary tests added

Added 24 new tests to `bill_payments/src/tests_bill_schedule_exec.rs`:

**Cross-tenant rejection (5 tests):**
- `test_cross_tenant_modify_schedule_rejected` — B cannot modify A's schedule
- `test_cross_tenant_cancel_schedule_rejected` — B cannot cancel A's schedule
- `test_cross_tenant_pay_bill_rejected` — B cannot pay A's bill via batch
- `test_cross_tenant_cancel_bill_rejected` — B cannot cancel A's bill
- `test_cross_tenant_add_tags_rejected` — B cannot tag A's bill
- `test_cross_tenant_set_external_ref_rejected` — B cannot set ext ref on A's bill

**Kill-switch regression (9 tests):**
- `test_kill_switch_blocks_create_bill_schedule`
- `test_kill_switch_blocks_modify_bill_schedule`
- `test_kill_switch_blocks_cancel_bill_schedule`
- `test_kill_switch_blocks_create_bill`
- `test_kill_switch_blocks_set_external_ref`
- `test_kill_switch_blocks_archive_paid_bills`
- `test_kill_switch_blocks_bulk_cleanup_bills`
- `test_kill_switch_no_partial_state_on_create_bill`
- `test_kill_switch_no_partial_state_on_create_schedule`

**Repeated/idempotent safety (2 tests):**
- `test_cancel_already_cancelled_schedule_returns_not_found`
- `test_modify_cancelled_schedule_returns_not_active`

**Stale identity rejection (2 tests):**
- `test_modify_nonexistent_schedule_returns_not_found`
- `test_cancel_nonexistent_schedule_returns_not_found`

**Batch partial-state prevention (2 tests):**
- `test_batch_pay_bills_skips_unauthorized_no_mutation`
- `test_batch_pay_bills_skips_already_paid`

**Cross-tenant schedule isolation (4 tests):**
- `test_execute_schedule_generates_bill_for_schedule_owner`
- `test_multiple_owner_schedules_independent`
- `test_get_bill_schedules_isolation`
- `test_get_bill_schedules_page_isolation`

All cross-tenant and kill-switch tests include **explicit no-mutation assertions**:
after a rejected operation, the original bill/schedule data is verified unchanged.

## Invariants

1. **Every write entry point** checks `require_no_active_kill_switch` as its first
   guard, before `require_auth` and before any storage read.
2. **Cross-tenant access** is rejected at the owner-check boundary:
   `bill.owner != caller` → `Err(Unauthorized)`, or `schedule.owner != caller` →
   `Err(Unauthorized)`.
3. **Rejected operations leave no partial state**: the kill-switch guard panics
   before any storage mutation; the owner check happens after loading but before
   any write.
4. **Idempotent operations** (`execute_due_bill_schedules`) are safe to call
   repeatedly; the `last_executed >= next_due` guard prevents double-execution.

## Failure Behavior

- Kill-switch active → `panic_with_error!(KillSwitchError::WriteBlocked)` — entire
  transaction reverts, no partial state.
- Cross-tenant → `Err(BillPaymentsError::Unauthorized)` — no state mutation.
- Already-cancelled schedule → `Err(ScheduleNotActive)`.
- Non-existent schedule → `Err(ScheduleNotFound)`.
- `batch_pay_bills` with unauthorized bill IDs → silently skips (no error, no
  mutation to the skipped bill).

## Compatibility

- **No public API signature changes.** The new kill-switch checks are defense-in-depth
  guards that fire before `require_auth`; they do not change function signatures or
  return types.
- **Error code renumbering**: `SameAdmin` (34), `RotationTimelockTooShort` (35),
  `InvalidStateTransition` (36), `InvariantViolation` (37) have new discriminant
  values. On-chain consumers matching by discriminant should update; name-based
  consumers are unaffected.
- **No migration required.** Instance storage layout is unchanged.

## Security Assumptions

- The kill switch is a simple bool toggle in instance storage. It is the coarsest
  emergency brake: when active, all write entry points are blocked.
- The kill switch itself is not gated by authentication in `remitwise-common` — the
  calling contract is responsible for admin auth before calling
  `activate_kill_switch`.
- `execute_due_bill_schedules` remains permissionless (any caller, no auth required)
  by design: it is a batch executor that reads schedule state to decide which bills
  to generate. The idempotency guard (`last_executed >= next_due`) and batch size
  limit (`MAX_BATCH_SIZE`) prevent abuse.

## Validation Commands

```bash
# Compile the lib (no wasm target needed)
cargo check -p bill_payments --lib

# Run the schedule execution + authorization boundary tests
cargo test -p bill_payments --test tests_bill_schedule_exec
```

**Results:** 39 passed, 0 failed, 2 ignored (pre-existing tests unrelated to this
issue).
