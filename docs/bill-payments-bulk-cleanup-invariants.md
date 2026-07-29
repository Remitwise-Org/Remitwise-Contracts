# bill-payments: `bulk_cleanup_bills` Destructive-Cleanup Accounting Contract

## Context

`BillPayments::bulk_cleanup_bills(caller, before_timestamp)` permanently
deletes every entry in `ARCH_BILL` whose `archived_at < before_timestamp`.
It then removes each deleted ID from the per-owner `ARCH_IDX` (archived bill
index) and refreshes `STOR_STAT` (storage stats).

Bulk destructive operations are the highest-risk class of accounting bug.
If `bulk_cleanup_bills` forgets to update an index, or accidentally touches the
active bill index (`OWN_IDX`) or the unpaid-totals map (`UNPD_TOT`), the cap
enforcement and reporting totals drift permanently with no error raised.

---

## Storage keys touched

| Key | Type | Written by cleanup? |
|-----|------|---------------------|
| `ARCH_BILL` | `Map<u32, ArchivedBill>` | ✅ Entries removed |
| `ARCH_IDX` | `Map<Address, Vec<u32>>` | ✅ Per-owner ID lists pruned |
| `STOR_STAT` | `StorageStats` | ✅ `archived_bills` decremented |
| `OWN_IDX` | `Map<Address, Vec<u32>>` | ❌ Must not be touched |
| `UNPD_TOT` | `Map<Address, i128>` | ❌ Must not be touched |
| `BILLS` | `Map<u32, Bill>` | ❌ Must not be touched |

---

## Invariants pinned by the test suite

### 1. Ownership scoping
`bulk_cleanup_bills` requires the caller to authenticate via
`caller.require_auth()`.  Calling without a valid auth context results in a
`HostError: Auth / InvalidAction` panic.  The archive scan itself is global
(all `ARCH_BILL` entries whose `archived_at < before_timestamp` are removed),
but each per-owner `ARCH_IDX` entry is updated independently.

### 2. `ARCH_IDX` accuracy (no stale entries)
After cleanup, `get_archived_bills` must return no IDs for deleted bills.
The per-owner Vec in `ARCH_IDX` is rebuilt without the removed IDs via
`index_remove_archived_batch`.

### 3. `OWN_IDX` unaffected
`get_owner_bill_count` (which reads `OWN_IDX`) must equal the same value
before and after cleanup.  Active bills are never in `ARCH_BILL` so they
cannot be deleted.

### 4. `UNPD_TOT` unaffected
`get_total_unpaid` must equal the same value before and after cleanup.
Archived bills were already removed from `UNPD_TOT` at archive time
(in `archive_paid_bills`); cleanup does not touch the totals map.

### 5. Exact count decrement
If `n` archived bills qualified, cleanup returns `Ok(n)` and the
owner's archive index shrinks by exactly `n`.  A `before_timestamp` equal to
`archived_at` does **not** qualify (strict less-than comparison).

### 6. Idempotency
Re-running cleanup with the same or larger `before_timestamp` after all
qualifying bills have been removed is a safe no-op returning `Ok(0)`.

### 7. Mixed-state correctness
When an owner has a mix of unpaid active bills, paid-but-not-archived bills,
and archived bills, cleanup removes only the archived subset.  The unpaid total
and active bill count are identical before and after.

### 8. Empty-set safety
Calling `bulk_cleanup_bills` when the archive is completely empty (no bills
ever archived, or all previously cleaned up) returns `Ok(0)` without panic.

### 9. Owner index emptied completely
When all archived bills for an owner are deleted, the owner's `ARCH_IDX`
entry is left completely clean.  `get_archived_bill` returns `None` for every
deleted ID.

### 10. Overflow safety
`i128` totals (`UNPD_TOT`) use saturating arithmetic and are not corrupted by
large-amount bills.  Cleanup of archived large-amount bills must not affect
the active unpaid totals in any way.

---

## Pause / auth gates

`bulk_cleanup_bills` calls `require_not_paused(env, pause_functions::ARCHIVE)`.

- If the **entire contract** is paused, returns `Err(ContractPaused)`.
- If the **`archive` function** is paused individually, returns
  `Err(FunctionPaused)`.

Both gates are asserted in the invariant test suite.

---

## Partial-cleanup semantics

`before_timestamp` is an **exclusive upper bound** on `archived_at`:

```
deleted iff:  bill.archived_at < before_timestamp
kept    iff:  bill.archived_at >= before_timestamp
```

This allows callers to clean up bills archived before a specific cutoff while
retaining more recent archives.

---

## Related tests

All invariant tests live in
`bill_payments/tests/bulk_cleanup_invariants.rs`:

| Test name | Invariant |
|-----------|-----------|
| `test_cleanup_does_not_touch_active_bills_or_unpaid_totals` | 1, 3, 4 |
| `test_cleanup_archive_count_decrements_exactly` | 2, 5 |
| `test_cleanup_idempotent_on_already_removed_bills` | 6 |
| `test_cleanup_mixed_paid_unpaid_archived_state` | 7 |
| `test_cleanup_empty_archive_is_noop` | 8 |
| `test_cleanup_empties_owner_archive_index_entirely` | 9 |
| `test_cleanup_overflow_safe_totals` | 10 |
| `test_cleanup_multi_owner_isolation` | 1, 2 |
| `test_cleanup_partial_timestamp_leaves_remainder` | 5, 2 |
| `test_cleanup_then_storage_stats_zero_archived` | 2 |
| `test_cleanup_requires_auth` | 1 (auth gate) |
| `test_cleanup_blocked_when_function_paused` | pause gate |
| `test_cleanup_blocked_when_contract_paused` | pause gate |

Run with:

```bash
cargo test -p bill_payments bulk_cleanup_bills -- --nocapture
# or the full invariant suite
cargo test -p bill_payments --test bulk_cleanup_invariants -- --nocapture
```
