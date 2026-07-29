# Insurance Policy Cap and Index Accounting

## Overview

The Insurance contract enforces a per-owner limit on active policies to prevent unbounded growth and lock-out scenarios. This document explains the active-count accounting via the `OWN_ACT` (KEY_OWNER_ACTIVE) index and how slot management works across policy lifecycle operations.

## Constant and Limits

**`MAX_POLICIES_PER_OWNER = 50`**

- Maximum number of active (non-archived) policies any owner can hold simultaneously.
- Enforced at policy creation time via `create_policy()`.
- The limit is **per owner**, not global, allowing multiple owners to each hold up to 50 active policies.

## Storage Keys and Indexes

### KEY_OWNER_ACTIVE (OWN_ACT)

**Type:** `Map<Address, u32>`

- **Purpose:** Tracks the count of active policies for each owner.
- **Invariant:** The value for an owner is strictly less than `MAX_POLICIES_PER_OWNER` or equals it when at capacity.
- **Updates:**
  - Incremented by +1 on `create_policy()` (successful creation).
  - Decremented by -1 on `deactivate_policy()` (only if transitioning from active → inactive).
  - Decremented by -1 on `archive_policy()` (only if the policy was active before archiving).
  - Incremented by +1 on `restore_policy()` (moving from archive back to active).

### KEY_OWNER_INDEX (OWN_IDX)

**Type:** `Map<Address, Vec<u32>>`

- **Purpose:** Lists all policy IDs (both active and inactive) owned by each address.
- **Relationship to OWN_ACT:** `OWN_IDX` is unbounded; it includes all policies ever created. Only the count of policies in `OWN_ACT` matters for cap enforcement.
- **Operations:**
  - Append policy ID on `create_policy()`.
  - **No removal** on deactivate/archive (policies remain in the index for pagination and history).

### KEY_POLICIES

**Type:** `Map<u32, InsurancePolicy>`

- **Purpose:** Active and inactive policy records.
- **Field:** `InsurancePolicy.active: bool`
  - `true` = actively counted toward the owner's `OWN_ACT` cap.
  - `false` = deactivated but not yet archived; still in `KEY_POLICIES` but not counted toward cap.
- **Updates:**
  - Created with `active = true` on `create_policy()`.
  - Changed to `active = false` on `deactivate_policy()`.
  - Removed entirely on `archive_policy()` (moved to `KEY_ARCHIVED`).

### KEY_ARCHIVED

**Type:** `Map<u32, ArchivedPolicy>`

- **Purpose:** Permanently archived policies (out of cap accounting).
- **Relationship to OWN_ACT:** Archived policies do **not** contribute to the owner's active count.
- **Restoration:** `restore_policy()` moves a policy back to `KEY_POLICIES` and increments `OWN_ACT` (subject to cap).

### KEY_EXT_REF_IDX

**Type:** `Map<String, u32>`

- **Purpose:** Lookup index mapping external references to policy IDs.
- **Invariant:** Only contains entries for active policies. Entries are removed when:
  - `deactivate_policy()` is called.
  - `archive_policy()` is called (whether the policy was active or inactive).
  - `set_external_ref()` updates or clears an external reference.

## Policy Lifecycle and Cap Effects

### 1. Create Policy

```
create_policy(owner, ..., external_ref) -> Result<u32, PolicyLimitExceeded>
```

- **Pre-check:** Read `OWN_ACT[owner]`. If `>= MAX_POLICIES_PER_OWNER`, return `Err(PolicyLimitExceeded)`.
- **On Success:**
  - Allocate new policy ID.
  - Create `InsurancePolicy` with `active = true`.
  - Append to `OWN_IDX[owner]`.
  - **Increment `OWN_ACT[owner]` by +1.**
  - Insert into `KEY_EXT_REF_IDX` if external_ref is provided.
  - Emit `PolicyCreatedEvent`.
  - Update `StorageStats.active_policies`.

**Slot Consumption:** +1 active slot.

### 2. Deactivate Policy

```
deactivate_policy(owner, policy_id) -> Result<bool, _>
```

- **Precondition:** Policy must exist and belong to the owner.
- **Idempotent:** If already inactive, no further decrements occur.
- **On Success (active → inactive transition only):**
  - Set `policy.active = false`.
  - Remove external_ref from `KEY_EXT_REF_IDX`.
  - **Decrement `OWN_ACT[owner]` by -1.**
  - Emit `PolicyDeactivatedEvent`.
  - Update `StorageStats.active_policies`.

**Slot Recovery:** Frees +1 active slot, but policy remains in `KEY_POLICIES` (not archived).

### 3. Archive Policy

```
archive_policy(owner, policy_id) -> Result<bool, Unauthorized>
```

- **Precondition:** Policy must exist; caller must be the owner.
- **Behavior:** Moves policy from `KEY_POLICIES` to `KEY_ARCHIVED`.
- **On Success:**
  - If policy was active:
    - Remove external_ref from `KEY_EXT_REF_IDX`.
    - **Decrement `OWN_ACT[owner]` by -1** (if active before archiving).
  - Move policy record to `KEY_ARCHIVED`.
  - Remove from `KEY_POLICIES`.
  - Update `StorageStats.active_policies` and `StorageStats.archived_policies`.

**Slot Recovery:** Frees +1 active slot if the policy was active; +0 if already inactive.

### 4. Restore Policy

```
restore_policy(owner, policy_id) -> bool
```

- **Precondition:** Policy must exist in `KEY_ARCHIVED` and belong to the owner.
- **Pre-check:** Read `OWN_ACT[owner]`. If `>= MAX_POLICIES_PER_OWNER`, return `false` (no slot available).
- **On Success:**
  - Move policy from `KEY_ARCHIVED` back to `KEY_POLICIES` with `active = true`.
  - Insert external_ref into `KEY_EXT_REF_IDX` (if present and not duplicated).
  - **Increment `OWN_ACT[owner]` by +1.**
  - Update `StorageStats.active_policies` and `StorageStats.archived_policies`.

**Slot Consumption:** +1 active slot (if restore succeeds).

## Security Assumptions

### Unbounded Growth Prevention

The `OWN_ACT` index **prevents** unbounded growth:

- Each owner can hold at most `MAX_POLICIES_PER_OWNER = 50` active policies.
- Archived policies do not count toward the cap.
- The contract rejects `create_policy()` if the owner is at or above the cap.

**Verification:** Tests confirm creation at cap (50) succeeds, at cap+1 fails with `PolicyLimitExceeded`.

### Lock-Out Prevention

Deactivate/archive operations **always succeed and free slots**:

- `deactivate_policy()` and `archive_policy()` never fail due to cap (only authorization).
- Owners can always free active slots by deactivating or archiving policies.
- `restore_policy()` fails gracefully if no slots are available (returns `false`).

**Verification:** Tests confirm that after reaching cap, deactivating one policy allows creating a new one.

### Index Consistency

The external-reference index (`KEY_EXT_REF_IDX`) is kept **consistent** with active status:

- Entries are only created for active policies.
- Entries are removed when a policy becomes inactive (deactivate, archive, or clear ref).
- Lookup via `get_policy_id_by_external_ref()` never returns stale IDs for inactive policies.

## Storage Stats Counters

**StorageStats** maintains two key counters:

```rust
pub struct StorageStats {
    pub active_policies: u32,      // Count of policies in KEY_POLICIES with active=true
    pub archived_policies: u32,    // Count of policies in KEY_ARCHIVED
    pub last_updated: u64,         // Timestamp of last update
}
```

### Update Rules

| Operation              | active_policies | archived_policies |
|------------------------|------------------|------------------|
| create_policy          | +1               | no change        |
| deactivate_policy      | -1 (if active)   | no change        |
| archive_policy         | -1 (if active)   | +1               |
| restore_policy         | +1               | -1               |

### Determinism

- Stats are updated **atomically** with each operation.
- Idempotent operations (e.g., deactivate of already-inactive policy) do not double-decrement.
- Storage stats always reflect the true count of active and archived policies.

## Pagination via get_active_policies()

The `get_active_policies(owner, cursor, limit) -> PolicyPage` function provides cursor-based pagination:

```rust
pub fn get_active_policies(
    env: Env,
    owner: Address,
    cursor: u32,           // Resume from this policy ID (0 = start)
    limit: u32,            // Page size (clamped to MAX_PAGE_LIMIT)
) -> PolicyPage {
    pub items: Vec<InsurancePolicy>,   // Active policies returned
    pub next_cursor: u32,              // Cursor for next page (0 = end)
    pub count: u32,                    // Number of items in this page
}
```

### Consistency with Active Index

- Iterates only over `OWN_IDX[owner]` (bounded by owner's policies, not all policies).
- Filters by `active = true` (excludes deactivated policies still in `KEY_POLICIES`).
- Results do **not** include archived policies (stored separately in `KEY_ARCHIVED`).
- Cursor is a policy ID; pages are ordered by policy ID numerically.

### Pagination Guarantees

- **Stable:** As long as no policies are deleted/archived during pagination, cursor moves are deterministic.
- **No duplicates:** Policy IDs are sorted and deduplicated per pagination call.
- **Bounded memory:** Page size is clamped to `MAX_PAGE_LIMIT = 50` to limit gas and memory usage.

## Event Emission

The contract emits events for all cap-affecting operations:

- `PolicyCreatedEvent` — policy creation (active=true, increments cap).
- `PolicyDeactivatedEvent` — deactivation (active=false, frees slot).
- `EVT_ARCHIVED` — archival transition (moves to archived storage).
- `EVT_RESTORED` — restoration from archive (re-consumes slot if successful).

Off-chain indexers can rely on these events to track active-count changes without polling storage.

## Testing Strategy

Comprehensive test coverage verifies:

1. **Cap Boundary Tests**
   - Create at cap (50) succeeds.
   - Create at cap+1 fails with `PolicyLimitExceeded`.
   - Boundary at 49, 50, 51 tested explicitly.

2. **Deactivate/Archive Slot Freeing**
   - Deactivate frees one slot; new policy can be created.
   - Archive (directly or after deactivate) frees one slot.
   - Archive of already-inactive policy frees slot (not double-counted).

3. **Restore with Cap Checking**
   - Restore at cap returns `false` (gracefully fails).
   - Restore after freeing slot succeeds and increments cap.
   - Restore after archiving works correctly.

4. **Pagination Consistency**
   - All pages together return same policies as full fetch.
   - No duplicates across pages.
   - Inactive policies excluded from pagination.
   - Archived policies excluded from pagination.

5. **Stats Determinism**
   - Active count matches number of policies with `active=true`.
   - Archived count matches number of policies in `KEY_ARCHIVED`.
   - Counters updated atomically with each operation.

## Example Scenarios

### Scenario 1: Create, Deactivate, Create Again

```rust
// Owner starts with 0 active policies
assert_eq!(get_active_count(owner), 0);

// Create policy 1
let id1 = create_policy(owner, ...) // OWN_ACT[owner] = 1
assert_eq!(get_active_count(owner), 1);

// Deactivate policy 1
deactivate_policy(owner, id1) // OWN_ACT[owner] = 0
assert_eq!(get_active_count(owner), 0);

// Create policy 2 (reuses the freed slot)
let id2 = create_policy(owner, ...) // OWN_ACT[owner] = 1
assert_eq!(get_active_count(owner), 1);
```

### Scenario 2: At Cap, Archive, Restore at New Cap

```rust
// Create 50 policies (at cap)
for _ in 0..50 { create_policy(owner, ...); }
assert_eq!(get_active_count(owner), 50);

// Archive policy 1 (moves to KEY_ARCHIVED, frees slot)
archive_policy(owner, id1)
assert_eq!(get_active_count(owner), 49);

// Create one more (now back at cap)
create_policy(owner, ...)
assert_eq!(get_active_count(owner), 50);

// Try to restore archived policy 1 (fails, at cap)
restore_policy(owner, id1) // Returns false
assert_eq!(get_active_count(owner), 50);

// Deactivate something to free a slot
deactivate_policy(owner, some_id)
assert_eq!(get_active_count(owner), 49);

// Now restore works
restore_policy(owner, id1) // Returns true
assert_eq!(get_active_count(owner), 50);
```

## Migration and Deployment Notes

- Existing deployments: Ensure `OWN_ACT` is properly initialized or populated from migration data.
- Backward compatibility: The cap is enforced at creation time; no existing policies are forcibly removed.
- Gas optimization: `owner_active_count()` is an O(1) lookup in `OWN_ACT`, not a full scan of policies.

## Related Documentation

- [Storage Layout](../STORAGE_LAYOUT.md) — Overview of all storage keys.
- [Access Control Matrix](../ACCESS_CONTROL_MATRIX.md) — Authorization rules for cap-related operations.
- [Threat Model](../THREAT_MODEL.md) — Security analysis of cap enforcement.
