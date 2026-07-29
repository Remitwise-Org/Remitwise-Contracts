# Owner-Indexed Pagination Design (SC-062)

## Overview

This document describes the per-owner bill caps and owner-indexed pagination implementation for the bill_payments contract. This eliminates scan-heavy reads that iterate over 1..=NEXT_ID gaps, keeping gas bounded and reducing DoS risk.

## Changes

### 1. Constants

- `MAX_BILLS_PER_OWNER = 1_000`: Maximum active bills per owner (archived bills don't count)
- `STORAGE_OWNER_INDEX = "OWN_IDX"`: Storage key for active bill index (Map<Address, Vec<u32>>)
- `STORAGE_ARCH_INDEX = "OWN_ARCH"`: Storage key for archived bill index (Map<Address, Vec<u32>>)

### 2. Error Codes

- `OwnerBillCapExceeded = 16`: Returned when owner tries to create a bill beyond MAX_BILLS_PER_OWNER

### 3. Storage Schema

**Active Bill Index:**
```
STORAGE_OWNER_INDEX: Map<Address, Vec<u32>>
```
- Maps each owner to a Vec of their active bill IDs
- IDs are stored in creation order (ID-ascending by construction)
- Updated on: create_bill, pay_bill (recurring), cancel_bill, archive_paid_bills

**Archived Bill Index:**
```
STORAGE_ARCH_INDEX: Map<Address, Vec<u32>>
```
- Maps each owner to a Vec of their archived bill IDs
- IDs are stored in archive order
- Updated on: archive_paid_bills, restore_bill, bulk_cleanup_bills

### 4. Index Helper Functions

**Private helpers:**
- `get_owner_bills(env, owner) -> Vec<u32>`: Returns active bill IDs for owner
- `get_owner_archived_bills(env, owner) -> Vec<u32>`: Returns archived bill IDs for owner
- `index_add_active(env, owner, bill_id)`: Appends bill_id to owner's active index
- `index_remove_active(env, owner, bill_id)`: Removes bill_id from owner's active index
- `index_add_archived(env, owner, bill_id)`: Appends bill_id to owner's archived index
- `index_remove_archived(env, owner, bill_id)`: Removes bill_id from owner's archived index

**Public query:**
- `get_owner_bill_count(env, owner) -> u32`: O(1) count of active bills for owner

### 5. Updated Functions

**create_bill:**
- Checks `get_owner_bills(&owner).len() >= MAX_BILLS_PER_OWNER` before creation
- Returns `OwnerBillCapExceeded` if cap is reached
- Calls `index_add_active(&owner, next_id)` after bill creation

**pay_bill:**
- When paying a recurring bill, the spawned next bill is added to the index via `index_add_active(&owner, next_id)`

**cancel_bill:**
- Calls `index_remove_active(&owner, bill_id)` after removing bill from storage

**archive_paid_bills:**
- For each archived bill:
  - Calls `index_remove_active(&owner, id)`
  - Calls `index_add_archived(&owner, id)`

**restore_bill:**
- Calls `index_remove_archived(&owner, bill_id)`
- Calls `index_add_active(&owner, bill_id)`

**bulk_cleanup_bills:**
- Collects (id, owner) pairs during scan
- Calls `index_remove_archived(&owner, id)` for each removed bill

**batch_pay_bills:**
- When paying recurring bills, spawned bills are added via `index_add_active(&owner, next_id)`

### 6. Pagination Functions (Rewritten)

All owner-scoped pagination functions now use the owner index instead of scanning 1..=NEXT_ID:

**get_unpaid_bills(owner, cursor, limit):**
- Reads `owner_ids = get_owner_bills(&owner)`
- Iterates `owner_ids` (skipping `id <= cursor`)
- Filters for `!bill.paid`
- O(owner_bills) instead of O(NEXT_ID)

**get_all_bills_for_owner(owner, cursor, limit):**
- Reads `owner_ids = get_owner_bills(&owner)`
- Iterates `owner_ids` (skipping `id <= cursor`)
- No additional filtering
- O(owner_bills) instead of O(NEXT_ID)

**get_archived_bills(owner, cursor, limit):**
- Reads `owner_ids = get_owner_archived_bills(&owner)`
- Iterates `owner_ids` (skipping `id <= cursor`)
- O(owner_archived) instead of O(NEXT_ID)

**get_bills_by_currency(owner, currency, cursor, limit):**
- Reads `owner_ids = get_owner_bills(&owner)`
- Iterates `owner_ids` (skipping `id <= cursor`)
- Filters for `bill.currency == normalized_currency`
- O(owner_bills) instead of O(NEXT_ID)

**get_unpaid_bills_by_currency(owner, currency, cursor, limit):**
- Reads `owner_ids = get_owner_bills(&owner)`
- Iterates `owner_ids` (skipping `id <= cursor`)
- Filters for `!bill.paid && bill.currency == normalized_currency`
- O(owner_bills) instead of O(NEXT_ID)

### 7. Ordering Guarantees

**ID-Ascending Order:**
- Active bill IDs are appended in creation order (which is ID-ascending by construction since NEXT_ID is monotonic)
- Pagination maintains ID-ascending order across pages
- Cursor stability: `cursor` points to the last returned item's ID

**Consistency:**
- Index updates are atomic with bill storage updates
- No gaps in the index (unlike 1..=NEXT_ID which has gaps from cancelled/archived bills)

## Performance Impact

**Before (scan-heavy):**
- `get_unpaid_bills`: O(NEXT_ID) - scans all bill IDs from 1 to NEXT_ID
- `get_all_bills_for_owner`: O(NEXT_ID)
- `get_archived_bills`: O(NEXT_ID)
- Gas cost grows with total bills across all owners

**After (index-based):**
- `get_unpaid_bills`: O(owner_bills) - only reads owner's bills
- `get_all_bills_for_owner`: O(owner_bills)
- `get_archived_bills`: O(owner_archived)
- Gas cost bounded by MAX_BILLS_PER_OWNER (1,000)

**DoS Mitigation:**
- Per-owner cap prevents any single owner from creating unbounded bills
- Index-based pagination prevents gas exhaustion from scanning large ID ranges
- Archived bills don't count toward the cap, allowing long-term history

## Migration Notes

**Backward Compatibility:**
- Existing bills will NOT have index entries until they are touched (paid, cancelled, archived)
- New deployments start with empty indexes
- Consider a one-time migration script to populate indexes for existing bills

**Migration Script (pseudo-code):**
```rust
pub fn migrate_populate_indexes(env: Env, admin: Address) {
    admin.require_auth();
    let bills: Map<u32, Bill> = env.storage().instance().get(&symbol_short!("BILLS")).unwrap();
    let archived: Map<u32, ArchivedBill> = env.storage().instance().get(&symbol_short!("ARCH_BILL")).unwrap();
    
    // Populate active index
    for (id, bill) in bills.iter() {
        index_add_active(&env, &bill.owner, id);
    }
    
    // Populate archived index
    for (id, bill) in archived.iter() {
        index_add_archived(&env, &bill.owner, id);
    }
}
```

## Testing

**Test Coverage:**
1. Cap enforcement: create MAX_BILLS_PER_OWNER + 1 bills, verify last fails
2. Index consistency: create/pay/cancel/archive/restore, verify index matches reality
3. Pagination correctness: verify ID-ascending order, cursor stability
4. Recurring bills: verify spawned bills are indexed
5. Batch operations: verify index updates for batch_pay_bills
6. Owner isolation: verify indexes don't leak across owners
7. Performance: benchmark pagination before/after (gas cost reduction)

## Security Considerations

1. **Cap Bypass:** Recurring bills spawned by pay_bill don't check the cap (by design - they're automatic). This is acceptable since the original bill was capped.
2. **Index Corruption:** If index and bill storage get out of sync, pagination may return stale/missing bills. Mitigation: atomic updates, migration script.
3. **DoS via Index Growth:** Bounded by MAX_BILLS_PER_OWNER per owner. Total index size is O(num_owners * MAX_BILLS_PER_OWNER).
4. **Gas Costs:** Index updates add O(owner_bills) cost to create/cancel/archive operations. This is acceptable since it eliminates O(NEXT_ID) pagination costs.

## Future Enhancements

1. **Sorted Index:** Maintain IDs in sorted order for binary search (currently linear scan)
2. **Composite Index:** Index by (owner, currency) for faster currency-filtered queries
3. **Lazy Migration:** Auto-populate index on first pagination call per owner
4. **Index Compaction:** Periodically rebuild indexes to remove gaps from cancelled bills
