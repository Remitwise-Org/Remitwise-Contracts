# Family Wallet: Transaction Archive Lifecycle

## Overview

The `family_wallet` contract maintains two transaction stores:

| Key | Purpose |
|-----|---------|
| `EXEC_TXS` | Completed multisig transactions awaiting archival |
| `ARCH_TX` | Archived historical records (bounded ring-buffer) |

Archiving moves entries from `EXEC_TXS` into `ARCH_TX` based on a caller-supplied retention cutoff timestamp. This keeps instance-storage rent bounded and provides a queryable audit trail.

---

## Archive Lifecycle

```
propose_transaction()
        │
        ▼
  PEND_TXS (pending)
        │
  sign_transaction() reaches threshold
        │
        ▼
  EXEC_TXS (executed metadata)
        │
  archive_old_transactions(before_timestamp)
        │
        ▼
  ARCH_TX (bounded archive, max MAX_ARCHIVE_ENTRIES)
```

---

## Key Functions

### `archive_old_transactions(caller, before_timestamp) → u32`

Moves entries from `EXEC_TXS` to `ARCH_TX` where `executed_at < before_timestamp`.

**Selection boundary:** The check is **strictly less-than**. An entry executed *at* `before_timestamp` is **not** archived — it remains in `EXEC_TXS`. This prevents accidentally archiving the most recent execution when the cutoff equals the current ledger time.

**Bounded growth invariant:** `ARCH_TX` is capped at `MAX_ARCHIVE_ENTRIES` (500). Before each insertion, if the archive is already at capacity, the entry with the **lowest `tx_id`** (oldest) is evicted. This ensures instance-storage rent never grows unboundedly regardless of how many transactions are executed over the contract's lifetime.

**Cutoff validation:** `before_timestamp` must satisfy `before_timestamp <= ledger.timestamp()`. A future cutoff is rejected with a panic to prevent accidentally archiving recent executions.

**Authorization:** Owner or Admin only.

**Returns:** The number of entries moved in this call.

**`STOR_STAT` update:** After archiving, `StorageStats.archived_transactions` is updated to reflect the current size of `ARCH_TX`.

### `get_archived_transactions(caller, limit) → Vec<ArchivedTransaction>`

Returns a page of archived entries ordered by ascending `tx_id`.

- `limit = 0` → uses `DEFAULT_ARCHIVE_PAGE_LIMIT` (20)
- `limit > MAX_ARCHIVE_PAGE_LIMIT` → clamped to `MAX_ARCHIVE_PAGE_LIMIT` (100)

**Authorization:** Owner or Admin only.

---

## Storage Bounds

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_ARCHIVE_ENTRIES` | 500 | Hard cap on `ARCH_TX` size |
| `DEFAULT_ARCHIVE_PAGE_LIMIT` | 20 | Default page size for reads |
| `MAX_ARCHIVE_PAGE_LIMIT` | 100 | Maximum page size for reads |

When `ARCH_TX` reaches `MAX_ARCHIVE_ENTRIES`, the oldest entry (lowest `tx_id`) is evicted before each new insertion. This is a **ring-buffer** eviction policy — oldest-first.

---

## Data Integrity

Each `ArchivedTransaction` record contains:

```rust
pub struct ArchivedTransaction {
    pub tx_id: u64,           // matches EXEC_TXS map key
    pub tx_type: TransactionType,
    pub proposer: Address,
    pub executed_at: u64,     // ledger timestamp at execution
    pub archived_at: u64,     // ledger timestamp at archival
}
```

If `meta.tx_id != map_key` in `EXEC_TXS`, the contract panics to prevent silent data corruption during archival.

---

## Security Considerations

- **Authorization-first:** `caller.require_auth()` is called before any storage reads.
- **Pause-aware:** Archiving is blocked when the contract is paused (`require_not_paused`).
- **No future cutoffs:** Prevents operators from accidentally archiving recent transactions by rejecting `before_timestamp > ledger.timestamp()`.
- **Idempotent:** Calling `archive_old_transactions` twice with the same cutoff is safe — entries already moved to `ARCH_TX` are no longer in `EXEC_TXS` and will not be double-counted.

---

## Test Coverage

All invariants are verified in `family_wallet/src/test.rs`:

| Test | Invariant Verified |
|------|--------------------|
| `test_archive_nothing_to_archive` | Empty `EXEC_TXS` → count 0, `ARCH_TX` empty |
| `test_archive_boundary_strictly_less_than` | Entry at cutoff not archived; entry before cutoff is archived |
| `test_archive_count_matches_entries_moved` | Return value equals entries moved |
| `test_archive_ordering_preserved` | `executed_at` timestamps preserved in archive |
| `test_archive_stor_stat_updated` | `STOR_STAT.archived_transactions` updated after archive |
| `test_archive_get_archived_limit_clamped` | `limit=0` uses default; `limit=9999` clamped |
| `test_archive_future_cutoff_rejected` | `before_timestamp > now` panics |
| `test_archive_re_pause_cancels_no_double_archive` | Second call with same cutoff returns 0 |
