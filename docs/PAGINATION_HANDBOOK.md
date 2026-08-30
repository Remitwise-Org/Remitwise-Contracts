# Pagination Handbook

> **Audience:** Contributors — engineers reviewing or implementing paginated reads in Remitwise contracts.
>
> **Scope:** Every paginated entrypoint in this codebase, how each is structured, and the rules reviewers use to verify correctness.

## Table of Contents

1. [Why Pagination Matters in Soroban](#why-pagination-matters-in-soroban)
2. [Universal Rules](#universal-rules)
3. [Paginated Entrypoints](#paginated-entrypoints)
4. [Request & Response Examples](#request--response-examples)
5. [Limit Enforcement](#limit-enforcement)
6. [Cursor Semantics](#cursor-semantics)
7. [Edge Cases](#edge-cases)
8. [Reviewer Checklist](#reviewer-checklist)
9. [Adding a New Paginated Read](#adding-a-new-paginated-read)

---

## Why Pagination Matters in Soroban

Soroban contracts execute inside a metered ledger environment. Every storage read costs CPU instructions and ledger entries. Returning an unbounded list in a single call can:

- Exceed the per-transaction CPU instruction limit
- Push the response beyond the XDR size cap
- Cause unnecessary storage rent charges

Pagination caps the work per invocation and lets callers walk large datasets across multiple transactions.

---

## Universal Rules

Every paginated read in this codebase follows these rules. **Reviewers must verify all of these before approving.**

| # | Rule | Why |
|---|------|-----|
| 1 | `limit` is normalized via `clamp_limit()` | Prevents unbounded reads, ensures predictable gas |
| 2 | Results beyond range return empty page, not an error | Callers detect end of list cleanly |
| 3 | Results are ordered deterministically | Callers can resume correctly across pages |
| 4 | Cursor/index is exclusive or absolute (documented) | Prevents duplicates and gaps across page boundaries |
| 5 | Return type is a `#[contracttype]` Page struct | Proper XDR serialization |

---

## Paginated Entrypoints

All 15 paginated functions across the Remitwise contracts are documented below. Each section shows exact parameter types, storage keys, ordering, and limit enforcement.


### `get_goals(owner, cursor, limit)` — Savings Goals

**File:** `savings_goals/src/lib.rs:1622`

**Purpose:** Returns a page of savings goals for an owner.

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `owner` | `Address` | Account whose goals to fetch |
| `cursor` | `u32` | Exclusive goal ID boundary (0 = start) |
| `limit` | `u32` | Max goals to return (clamped to `MAX_PAGE_LIMIT = 50`) |

**Returns:** `GoalPage { items: Vec<SavingsGoal>, next_cursor: u32, count: u32 }`

**Storage reads:**
- Index: `DataKey::OwnerGoals(owner)` → `Vec<u32>` of goal IDs
- Items: `DataKey::Goal(goal_id)` for each ID in range

**Ordering:** Ascending by goal ID (creation order within owner index).

**Limit enforcement:**
```rust
let limit = clamp_limit(limit);  // 0→20, 1-50→pass, >50→50
```

**Cursor semantics (EXCLUSIVE):**
- `cursor = 0`: Start from first goal
- `cursor = N` (where N exists): Skip to goal with ID > N, then take `limit` items
- Returns `next_cursor = ID of last returned item` when more pages exist
- Returns `next_cursor = 0` when final page reached

**Edge cases:**
- `cursor` not in index: Panics with "Invalid cursor"
- `cursor` from different owner: Panics with "Pagination index owner mismatch"
- All goals archived: Returns empty page

**Tests:** `savings_goals/tests/stress_tests.rs`, `savings_goals/tests/gas_bench.rs`, `savings_goals/src/test.rs:1913+`

---

### `get_goals_by_tag(owner, tag, cursor, limit)` — Savings Goals

**File:** `savings_goals/src/lib.rs:1695`

**Purpose:** Returns a page of savings goals matching a tag, for an owner.

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `owner` | `Address` | Account whose goals to fetch |
| `tag` | `String` | Tag filter (canonicalized before lookup) |
| `cursor` | `u32` | Exclusive goal ID boundary (0 = start) |
| `limit` | `u32` | Max goals to return (clamped to `MAX_PAGE_LIMIT = 50`) |

**Returns:** `GoalPage { items: Vec<SavingsGoal>, next_cursor: u32, count: u32 }`

**Storage reads:**
- Index: `DataKey::TagIndex(owner, canonical_tag)` → `Vec<u32>` of goal IDs with this tag
- Items: `DataKey::Goal(goal_id)` for each ID in range

**Ordering:** Ascending by goal ID within tag index.

**Cursor semantics:** EXCLUSIVE, same as `get_goals()`.

**Tag normalization:** Tags are canonicalized (trimmed, case-folded) before lookup. Queries are case-insensitive.

**Edge cases:**
- Tag not found for owner: Returns empty page (no panic)
- `cursor` not in tag index: Panics with "Invalid cursor"

**Tests:** `savings_goals/src/test.rs:6557+` (tag-based pagination tests)

---

### `get_archived_goals_page(owner, cursor, limit)` — Savings Goals

**File:** `savings_goals/src/lib.rs:1904`

**Purpose:** Returns a page of archived savings goals for an owner.

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `owner` | `Address` | Account whose archived goals to fetch |
| `cursor` | `u32` | Exclusive archived goal ID boundary (0 = start) |
| `limit` | `u32` | Max goals to return (clamped to `MAX_PAGE_LIMIT = 50`) |

**Returns:** `ArchivedGoalPage { items: Vec<ArchivedSavingsGoal>, next_cursor: u32, count: u32 }`

**Storage reads:**
- Index: `DataKey::ArchivedGoalsIndex(owner)` → `Vec<u32>` of archived goal IDs
- Items: `DataKey::ArchivedGoal(goal_id)` for each ID in range

**Ordering:** Ascending by archived goal ID.

**Cursor semantics:** EXCLUSIVE, same as `get_goals()`.

**Alias:** `get_archived_goals()` at line 1968 is a convenience wrapper.

**Edge cases:**
- No archived goals: Returns empty page
- `cursor` not in archived index: Panics with "Archived pagination index out of sync"

**Tests:** `savings_goals/src/test.rs:2170+` (archived goal pagination tests)


### `get_unpaid_bills(owner, cursor, limit)` — Bill Payments

**File:** `bill_payments/src/lib.rs:2153`

**Purpose:** Returns a page of unpaid bills for an owner.

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `owner` | `Address` | Account whose unpaid bills to fetch (requires auth) |
| `cursor` | `u32` | Exclusive bill ID boundary (0 = start) |
| `limit` | `u32` | Max bills to return (clamped to `MAX_PAGE_LIMIT = 50`) |

**Returns:** `BillPage { items: Vec<Bill>, next_cursor: u32, count: u32 }`

**Storage reads:**
- Index: `DataKey::OwnerBills(owner)` → `Vec<u32>` of bill IDs for owner
- Items: Map `BILLS` lookup + per-bill fetch

**Ordering:** Ascending by bill ID.

**Filters:** Only bills where `bill.paid == false`.

**Cursor semantics (EXCLUSIVE):**
- Logic: `for id in owner_ids { if id <= cursor { continue; } ... }`
- Returns `next_cursor = ID of last returned item` when more pages exist
- Returns `next_cursor = 0` when no more unpaid bills

**Limit enforcement:**
```rust
let limit = clamp_limit(limit);  // 0→20, 1-50→pass, >50→50
```

**Edge cases:**
- No unpaid bills: Returns empty page with `next_cursor = 0`
- All bills paid: Returns empty page
- `cursor` beyond max bill ID: Returns empty page (safe default)

**Tests:** `bill_payments/tests/stress_tests.rs`, gas benchmarks

---

### `get_unpaid_bills_by_currency(owner, currency, cursor, limit)` — Bill Payments

**File:** `bill_payments/src/lib.rs:3286`

**Purpose:** Returns a page of unpaid bills for an owner, filtered by currency (double-predicate pagination).

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `owner` | `Address` | Account whose bills to fetch (requires auth) |
| `currency` | `String` | Currency filter (normalized to uppercase) |
| `cursor` | `u32` | Exclusive bill ID boundary (0 = start) |
| `limit` | `u32` | Max bills to return (clamped to `MAX_PAGE_LIMIT = 50`) |

**Returns:** `BillPage { items: Vec<Bill>, next_cursor: u32, count: u32 }`

**Storage reads:**
- Index: Per-`(owner, currency)` index → `Vec<u32>` of bill IDs
- Items: Map `BILLS` lookup + per-bill fetch

**Ordering:** Ascending by bill ID within the currency index.

**Filters:** 
- `bill.owner == owner`
- `bill.paid == false`
- `bill.currency == normalized(currency)`

**Security:** `owner.require_auth()` enforced at function entry — the per-`(owner, currency)`
index is scoped to `owner`, so no cross-owner leakage can occur via cursor manipulation.

**Currency normalization:**
1. Trim leading/trailing ASCII whitespace
2. Convert to uppercase
3. Empty string → `"XLM"`

Example: `" usdc "` → `"USDC"`, `""` → `"XLM"`

**Cursor semantics (EXCLUSIVE):** Same as `get_unpaid_bills()`.

**Limit enforcement:** Same as `get_unpaid_bills()`.

**Implementation detail:** Uses per-`(owner, currency)` index instead of full owner index for O(bills_in_currency) traversal vs O(all_owner_bills).

**Edge cases:**
- Currency not found: Returns empty page (no panic)
- `cursor` points to archived bill: Skipped cleanly (missing entries ignored)
- No unpaid bills in currency: Returns empty page with `next_cursor = 0`

**Tests:** `bill_payments/tests/unpaid_by_currency_pagination.rs` (14 comprehensive test cases), `docs/bill-payments-unpaid-by-currency-pagination.md`

---

### `get_bills_by_currency(owner, currency, cursor, limit)` — Bill Payments

**File:** `bill_payments/src/lib.rs`

**Purpose:** Returns a page of ALL bills (paid + unpaid) for `owner` that match `currency`.

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `owner` | `Address` | Account whose bills to fetch (requires auth) |
| `currency` | `String` | Currency filter (normalized to uppercase) |
| `cursor` | `u32` | Exclusive bill ID boundary (0 = start) |
| `limit` | `u32` | Max bills to return (clamped to `MAX_PAGE_LIMIT = 50`) |

**Returns:** `BillPage { items: Vec<Bill>, next_cursor: u32, count: u32 }`

**Ordering:** Ascending by bill ID within the currency index.

**Filters:** `bill.owner == owner`, `bill.currency == normalized(currency)` (paid and unpaid both included).

**Cursor semantics (EXCLUSIVE):** Same as `get_unpaid_bills()`.

**Security:** `owner.require_auth()` enforced at function entry — the per-`(owner, currency)`
index is scoped to `owner`, so no cross-owner leakage can occur via cursor manipulation.

---

### `get_archived_bills(owner, cursor, limit)` / `get_archived_bills_page(owner, cursor, limit)` — Bill Payments

**File:** `bill_payments/src/lib.rs`

**Purpose:** Returns a page of archived bills for `owner`. Two equivalent entrypoints exist
for this query (`get_archived_bills` is the original entrypoint, `get_archived_bills_page`
is a later, more thoroughly documented one); both share identical cursor, limit, and
ordering semantics and return the same `ArchivedBillPage` shape.

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `owner` | `Address` | Account whose archived bills to fetch (requires auth) |
| `cursor` | `u32` | Exclusive bill ID boundary (0 = start) |
| `limit` | `u32` | Max items to return (clamped to `MAX_PAGE_LIMIT = 50`) |

**Returns:** `ArchivedBillPage { items: Vec<ArchivedBill>, next_cursor: u32, count: u32 }`

**Storage reads:** Per-owner `ARCH_IDX` index → `Vec<u32>` of archived bill IDs; `ARCH_BILL` map lookup per ID.

**Ordering:** Strictly ascending by bill ID.

**Cursor semantics (EXCLUSIVE):** Same as `get_unpaid_bills()`.

**Security:** `owner.require_auth()` enforced at function entry — the archived-bill index is
scoped to `owner`, so no cross-owner leakage can occur via cursor manipulation.

---

### `get_bill_schedules_page(owner, cursor, limit)` — Bill Payments

**File:** `bill_payments/src/lib.rs` (added in Issue #1751)

**Purpose:** Returns a deterministic, cursor-paginated page of recurring bill schedules for an owner.

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `owner` | `Address` | Account whose schedules to fetch (requires auth) |
| `cursor` | `u32` | Exclusive schedule ID boundary (0 = start) |
| `limit` | `u32` | Max schedules to return (clamped to `MAX_PAGE_LIMIT = 50`) |

**Returns:** `BillSchedulePage { items: Vec<BillSchedule>, next_cursor: u32, count: u32 }`

**Storage reads:**
- Index: `STORAGE_OWNER_BSCH_IDX` (`symbol_short!("OWN_BSCH")`) → `Map<Address, Vec<u32>>` of owner → schedule IDs
- Items: `STORAGE_BSCHEDS` (`symbol_short!("BSCHEDS")`) → `Map<u32, BillSchedule>` for each ID in range

**Ordering:** Ascending by schedule ID (creation order within owner index).

**Limit enforcement:**
```rust
let effective_limit = clamp_limit(limit);  // 0→20, 1-50→pass, >50→50
```

**Cursor semantics (EXCLUSIVE):**
- `cursor = 0`: Start from first schedule
- `cursor = N` (where N exists): Skip to schedules with ID > N, then take `effective_limit` items
- Returns `next_cursor = ID of last returned item` when more pages exist
- Returns `next_cursor = 0` when final page reached

**Edge cases:**
- No schedules for owner: Returns empty page with `next_cursor = 0` (not an error)
- `cursor` beyond max schedule ID: Returns empty page with `next_cursor = 0`
- Cancelled schedules (removed from owner index): Never appear in results

**Security:**
- `owner.require_auth()` enforced at function entry
- Per-owner index means cursor values from one owner cannot leak another owner's schedules

**Prior art:** `get_bill_schedules()` (same file) returns an unbounded `Vec<BillSchedule>` and is retained for backward compatibility. Prefer `get_bill_schedules_page()` for all new callers.

**Tests:** `bill_payments/tests/tests_recurring.rs` — 13 regression tests covering empty owner, single-page, first/second page cursors, exact-fit, out-of-range cursor, cursor at last ID, idempotent repeat, limit=0 normalisation, large limit clamping, full traversal no duplicates, owner isolation, cancelled schedule exclusion, concurrent insert stability, and field integrity.

---


### `get_active_policies(owner, cursor, limit)` — Insurance

**File:** `insurance/src/lib.rs:742`

**Purpose:** Returns a page of active insurance policies for an owner.

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `owner` | `Address` | Account whose policies to fetch |
| `cursor` | `u32` | Exclusive policy ID boundary (0 = start) |
| `limit` | `u32` | Max policies to return (clamped to `MAX_PAGE_LIMIT = 50`) |

**Returns:** `Result<PolicyPage { items: Vec<u32>, next_cursor: u32, count: u32 }, InsuranceError>`

**Note:** Returns policy IDs only, not full Policy objects.

**Storage reads:**
- Index: `DataKey::OwnerPolicies(owner)` → `Vec<u32>` of policy IDs
- Items: `DataKey::Policy(id)` for active policy lookup

**Ordering:** Ascending by policy ID.

**Filters:** Only policies where `policy.active == true`.

**Cursor semantics (EXCLUSIVE):**
- Logic: `for id in owner_ids { if id > cursor { ... } }`
- Returns `next_cursor = ID of last returned item` when more pages exist
- Returns `next_cursor = 0` when final page reached

**Limit enforcement:**
```rust
let lim = if limit == 0 {
    DEFAULT_PAGE_LIMIT
} else if limit > MAX_PAGE_LIMIT {
    MAX_PAGE_LIMIT
} else {
    limit
};
```

**Edge cases:**
- No active policies: Returns empty page with `next_cursor = 0`
- All policies deactivated: Returns empty page
- `cursor` beyond max policy ID: Returns empty page

**Tests:** `insurance/tests/...`

---

### `get_deactivated_policies(owner, cursor, limit)` — Insurance

**File:** `insurance/src/lib.rs:799`

**Purpose:** Returns a page of deactivated insurance policies for an owner.

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `owner` | `Address` | Account whose deactivated policies to fetch |
| `cursor` | `u32` | Exclusive policy ID boundary (0 = start) |
| `limit` | `u32` | Max policies to return (clamped to `MAX_PAGE_LIMIT = 50`) |

**Returns:** `Result<PolicyPage { items: Vec<u32>, next_cursor: u32, count: u32 }, InsuranceError>`

**Storage reads:** Same as `get_active_policies()`, but filtered for deactivated policies.

**Filters:** Only policies where `policy.active == false`.

**Cursor semantics (EXCLUSIVE):** Same as `get_active_policies()`.

**Limit enforcement:** Uses `clamp_limit()` from `remitwise-common`.

**Edge cases:** Same as `get_active_policies()` but for deactivated subset.

---


### `get_audit_log(from_index, limit)` — Remittance Split

**File:** `remittance_split/src/lib.rs:1856`

**Purpose:** Returns a page of audit log entries (oldest-to-newest).

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `from_index` | `u32` | Absolute zero-based array index (NOT exclusive cursor) |
| `limit` | `u32` | Max entries to return (clamped to `MAX_PAGE_LIMIT = 50`) |

**Returns:** `AuditPage { items: Vec<AuditEntry>, next_cursor: u32, count: u32 }`

**Storage reads:**
- Log: Instance storage key `symbol_short!("AUDIT")` → `Vec<AuditEntry>`
- Direct array indexing: `[from_index..from_index + limit]`

**Ordering:** Oldest-to-newest within rotating log window.

**Cursor semantics (ABSOLUTE INDEX, not exclusive):**
- `from_index` is a direct array position, not a boundary
- Returns `next_cursor = from_index + items.len()` for next page
- Returns `next_cursor = 0` when final page reached or no more entries

**Limit enforcement:**
```rust
let cap = clamp_limit(limit);  // 0→20, 1-50→pass, >50→50
```

**Safety:** Uses saturating arithmetic (`saturating_add`, `.min()`) to prevent overflow panics.

**Edge cases:**
- `from_index >= log.len()`: Returns empty page with `next_cursor = 0`
- Empty log: Returns empty page
- `from_index + limit` overflows: Handled by `.min()` and saturating arithmetic

**Pagination contract:** Documented in `docs/ORCHESTRATOR_SIGNING.md` (audit log section).

**Tests:** `remittance_split/tests/stress_test_large_amounts.rs:357+` (schedule pagination ordering tests)

---

### `get_remittance_schedules_page(owner, from_index, limit)` — Remittance Split

**File:** `remittance_split/src/lib.rs:2861`

**Purpose:** Returns a page of remittance schedules for an owner (cursor-based pagination).

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `owner` | `Address` | Account whose schedules to fetch |
| `from_index` | `u32` | Cursor for pagination (semantics per contract) |
| `limit` | `u32` | Max schedules to return (clamped to `MAX_PAGE_LIMIT = 50`) |

**Returns:** `SchedulePage { items: Vec<RemittanceSchedule>, next_cursor: u32, count: u32 }`

**Storage reads:** Owner-indexed schedule storage.

**Ordering:** Ascending by schedule ID (deterministic, stable across pages).

**Cursor semantics:** Documented as stable cursor-based pagination.

**Limit enforcement:** Clamped via `clamp_limit()`.

**Guarantees:**
- Deterministic: identical `(owner, from_index, limit)` always returns same page
- Enables reliable replay by audit consumers

**Tests:** `remittance_split/tests/stress_test_large_amounts.rs:423+` (stable cursor tests)

---

### `get_member_addresses_page(cursor, limit)` — Family Wallet

**File:** `family_wallet/src/lib.rs:1923`

**Purpose:** Returns a page of member addresses (permissionless read).

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `cursor` | `u32` | Iteration offset (skip first `cursor` members) |
| `limit` | `u32` | Max addresses to return (capped to `MAX_MEMBER_PAGE_LIMIT`) |

**Returns:** `MemberAddressPage { items: Vec<Address>, next_cursor: u32, count: u32 }`

**Storage reads:**
- Members map: Instance storage `symbol_short!("MEMBERS")` → `Map<Address, FamilyMember>`
- Direct iteration: skip first `cursor` entries

**Ordering:** Map iteration order (not stable across rebalancing).

**Cursor semantics (OFFSET, not exclusive ID):**
- `cursor = 0`: Start from first member
- `cursor = N`: Skip first N members, start from member N
- Logic: iterate, skip first `cursor`, take `limit` items
- Returns `next_cursor = cursor + items.len()` to reach next batch
- Returns `next_cursor = 0` when final page reached

**Limit enforcement:**
- Uses `DEFAULT_MEMBER_PAGE_LIMIT` (not standard 20)
- Uses `MAX_MEMBER_PAGE_LIMIT` (not standard 50)
- Custom clamping: `limit.min(MAX_MEMBER_PAGE_LIMIT)`

**Safety:** Saturating arithmetic on cursor advancement: `cursor.saturating_add(1)` and `cursor.saturating_add(items.len())`.

**Edge cases:**
- No members: Panics with "Wallet not initialized"
- `cursor >= member_count`: Returns empty page with `next_cursor = 0`
- Wallet not initialized: Panics (by design)

**Note:** Different pagination pattern than other contracts (offset-based via iteration count, not exclusive cursor by ID).

---


### `get_archived_reports_page(user, cursor, limit)` — Reporting

**File:** `reporting/src/lib.rs:2149`

**Purpose:** Returns a page of archived financial reports for a user (bounded pagination).

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `user` | `Address` | Account whose archived reports to fetch |
| `cursor` | `u32` | Exclusive report boundary (0 = start) |
| `limit` | `u32` | Max reports to return (clamped to `MAX_PAGE_LIMIT = 50`) |

**Returns:** `ArchivedPage { items: Vec<ArchivedReport>, next_cursor: u32, count: u32 }`

**Storage reads:** Per-user archived reports.

**Ordering:** Deterministic order (enforced by page reader).

**Cursor semantics (EXCLUSIVE):** Same as goal/bill pagination.

**Limit enforcement:** Clamped via `clamp_limit()` from `remitwise-common`.

**Bound enforcement:** Issue #832 introduced a bound: archived reports are paginated up to `MAX_PAGE_LIMIT` per call, preventing unbounded reads.

**Deprecation note:** Legacy `get_archived_reports()` is deprecated. Use `get_archived_reports_page()` instead.

**Edge cases:**
- No archived reports: Returns empty page with `next_cursor = 0`
- User has many archived reports: Pagination required (caller must loop)

**Tests:** `reporting/src/tests_archived_pagination_bound.rs` (8 tests covering bound, isolation, cursor, etc.)

**Related docs:** `docs/bill-payments-unpaid-by-currency-pagination.md` (similar double-predicate patterns)

---

### `get_audit_log(from_index, limit)` — Orchestrator

**File:** `orchestrator/src/lib.rs:837`

**Purpose:** Returns a page of audit log entries (oldest-to-newest).

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `from_index` | `u32` | Absolute zero-based array index |
| `limit` | `u32` | Max entries to return (clamped to `MAX_AUDIT_ENTRIES`) |

**Returns:** `Vec<AuditEntry>` (raw vector, NOT wrapped in page struct)

**Storage reads:**
- Log: Instance storage key `symbol_short!("AUDIT")` → `Vec<AuditEntry>`
- Direct array indexing: `[from_index..]`

**Ordering:** Oldest-to-newest.

**Cursor semantics (ABSOLUTE INDEX):**
- `from_index` is a direct array position
- Returns empty vec when `from_index >= log.len()`

**Limit enforcement:** Clamped to `MAX_AUDIT_ENTRIES` (not standard `MAX_PAGE_LIMIT`).

**Safety:**
- Local `clamp_limit()` function (Line 1610): `0 → 20`, `>MAX → MAX`
- Uses saturating arithmetic

**Edge cases:**
- Empty log: Returns empty vec
- `from_index >= log.len()`: Returns empty vec
- `limit = 0`: Returns empty vec

**Note:** Different return type than other audit logs (no wrapper struct). This asymmetry with remittance_split's `AuditPage` should be resolved in a future refactor.

**Tests:** `orchestrator/src/test.rs:432+` (audit log pagination no-duplicates test)

---


---

## Request & Response Examples

### Pattern 1: Exclusive Cursor (Goals, Bills, Policies)

**Fetching the first page:**
```rust
let owner = Address::random(&env);
let page_1 = client.get_goals(&env, &owner, &0u32, &10u32);
// Returns: GoalPage with up to 10 goals, IDs in ascending order
// next_cursor = ID of last returned goal (or 0 if < 10 items)
```

**Fetching the next page:**
```rust
let page_2 = client.get_goals(&env, &owner, &page_1.next_cursor, &10u32);
// Skips all goals with ID <= page_1.next_cursor
// Returns: Next 10 goals (or fewer if nearing end)
```

**Detecting end of list:**
```rust
let mut cursor = 0u32;
loop {
    let page = client.get_goals(&env, &owner, &cursor, &50u32);
    if page.items.is_empty() {
        break;  // Reached the end
    }
    process_goals(&page.items);
    cursor = page.next_cursor;
    if cursor == 0 {
        break;  // Reached the final page
    }
}
```

### Pattern 2: Absolute Index (Audit Logs)

**Fetching the first page:**
```rust
let page_1 = client.get_audit_log(&env, &0u32, &50u32);
// Returns: AuditPage with entries at indices [0..49]
// next_cursor = 50 (or 0 if < 50 items exist)
```

**Fetching the next page:**
```rust
let page_2 = client.get_audit_log(&env, &page_1.next_cursor, &50u32);
// Returns: Entries at indices [50..99]
```

### Pattern 3: Offset-Based (Family Wallet Members)

**Fetching the first page:**
```rust
let page_1 = client.get_member_addresses_page(&env, &0u32, &20u32);
// Returns: First 20 members (or fewer if fewer exist)
// next_cursor = 0 + items.len() (or 0 if end reached)
```

**Fetching the next page:**
```rust
let page_2 = client.get_member_addresses_page(&env, &page_1.next_cursor, &20u32);
// Skips first page_1.next_cursor members, returns next 20
```

### Safe Pagination with Out-of-Range Cursors

**Out-of-range cursor returns empty page (not an error):**
```rust
let page = client.get_goals(&env, &owner, &999_999u32, &10u32);
assert_eq!(page.items.len(), 0);
assert_eq!(page.next_cursor, 0);
// Safe default — caller detects end of list
```

---

## Limit Enforcement

### Standard Limit Clamping

The majority of contracts use `clamp_limit()` from `remitwise-common`:

```rust
pub fn clamp_limit(limit: u32) -> u32 {
    if limit == 0 {
        DEFAULT_PAGE_LIMIT     // 20
    } else if limit > MAX_PAGE_LIMIT {
        MAX_PAGE_LIMIT         // 50
    } else {
        limit
    }
}
```

**Result table:**

| Input `limit` | Clamped to | Why |
|---------------|------------|-----|
| `0` | `20` | Caller did not specify; use default |
| `1` | `1` | Passed through unchanged |
| `25` | `25` | Passed through unchanged |
| `50` | `50` | Passed through unchanged (inclusive upper bound) |
| `51` | `50` | Clamped down to max |
| `u32::MAX` | `50` | Extremely large input clamped directly |

### Custom Limit Clamping

**Family Wallet** uses custom limits:

```rust
let capped_limit = if limit == 0 {
    DEFAULT_MEMBER_PAGE_LIMIT
} else {
    limit.min(MAX_MEMBER_PAGE_LIMIT)
};
```

**Contracts using custom limits:**
- `family_wallet/src/lib.rs:1923` — member pagination uses `DEFAULT_MEMBER_PAGE_LIMIT`, `MAX_MEMBER_PAGE_LIMIT`

### Enforcement Pattern

Every paginated function MUST apply clamping **at the function entry point**, before any storage reads:

```rust
pub fn paginated_read(env: Env, cursor: u32, limit: u32) -> Page {
    let limit = clamp_limit(limit);  // ← Apply clamping first!
    
    // ... rest of function uses clamped `limit`
}
```

This ensures gas costs are predictable regardless of caller input.

---

## Cursor Semantics

### Type 1: Exclusive Cursor by ID

**Used by:** savings_goals, bill_payments, insurance, reporting

**How it works:**
```rust
for id in owner_ids.iter() {
    if id <= cursor {
        continue;  // Skip items with ID <= cursor
    }
    // Process item
}
```

**Semantics:**
- `cursor = 0`: Start from first item
- `cursor = N`: Skip to items with ID > N
- `next_cursor = ID of last returned item` (when more pages exist)
- `next_cursor = 0` (when final page reached)

**Why exclusive:** Prevents duplicates when items are deleted. If cursor were inclusive, a deleted item's ID could be reused, causing the next page to start mid-way through previously returned items.

**Example with deletions:**
```
Initial IDs: [1, 2, 3, 4, 5]

Page 1 (cursor=0, limit=2):  → [1, 2]  next_cursor=2
Delete ID 2
Page 2 (cursor=2, limit=2):  → [3, 4]  (skips 2 cleanly, even though deleted)
```

### Type 2: Absolute Index

**Used by:** remittance_split audit log, orchestrator audit log

**How it works:**
```rust
let start = from_index;
let end = from_index.saturating_add(limit).min(log.len());
for i in start..end {
    if let Some(entry) = log.get(i) {
        items.push_back(entry);
    }
}
let next_cursor = if end < log.len() { end } else { 0 };
```

**Semantics:**
- `from_index` is a direct array position `[0, log.len())`
- `next_cursor = from_index + items.len()` (or 0 if final)
- Caller must handle `from_index >= log.len()` → returns empty

**Why absolute:** Audit logs are immutable once written. No deletions occur, so absolute indexing is safe and simple.

### Type 3: Offset via Iteration Count

**Used by:** family_wallet member pagination

**How it works:**
```rust
for (address, _) in members.iter() {
    if seen < cursor {
        seen = seen.saturating_add(1);
        continue;  // Skip first `cursor` members
    }
    // Process member
}
let next_cursor = if has_more {
    cursor.saturating_add(items.len())
} else {
    0
};
```

**Semantics:**
- `cursor` is an iteration count, not an ID
- `next_cursor = cursor + items.len()` (how many to skip next)
- Caller must re-iterate to reach the next page (inefficient)

**Why different:** Map iteration order is not stable (can change when members are added/removed). Offset-based pagination is less efficient but avoids relying on insertion order.

---


---

## Edge Cases

All paginated functions must handle these edge cases correctly:

| Scenario | Behaviour | Why | Tested |
|----------|-----------|-----|--------|
| `cursor/index = 0, limit = 0` | Empty page (0 items clamped to 20, but no items fetched) | Caller may request 0 intentionally | ✅ |
| `cursor/index > item_count` | Empty page, `next_cursor = 0` | Caller detecting end of list | ✅ |
| `limit = 0` | Clamped to `DEFAULT_PAGE_LIMIT` (20) | Caller didn't specify | ✅ |
| `limit > MAX_PAGE_LIMIT` | Clamped to `MAX_PAGE_LIMIT` (50) | Prevents unbounded reads | ✅ |
| `limit = u32::MAX` | Clamped to `MAX_PAGE_LIMIT` (50) | Saturating arithmetic handles overflow | ✅ |
| `start + limit` overflows `u32` | Handled by saturating arithmetic or `.min()` | Prevents panics | ✅ |
| Single item in store | Returns vec of length 1 | Boundary case | ✅ |
| Empty index | Returns empty page, `next_cursor = 0` | No items yet | ✅ |
| Cursor/index from prior owner | Panics (goal/bill) or returns empty (other) | Owner isolation, security | ✅ |
| Invalid cursor (not in index) | Panics with "Invalid cursor" | Detects stale cursors | ✅ |
| Archived/deleted item in range | Skipped cleanly (item fetch returns None) | Sparse indices handled | ✅ |
| Tag not found | Returns empty page (no panic) | Tag filter has no matches | ✅ |
| Currency not found | Returns empty page (no panic) | Currency filter has no matches | ✅ |
| Map not initialized | Panics with "not initialized" | By design (fail early) | ✅ |
| Storage key collision | Cannot happen (keys are type-safe in Rust) | XDR serialization prevents collision | ✅ |

**Test files verifying edge cases:**

- `savings_goals/tests/stress_tests.rs` — 200+ goals stress test, pagination correctness
- `savings_goals/tests/gas_bench.rs` — first-page (n=50,200,1000), last-page benchmarks
- `savings_goals/src/test.rs` — cursor validation, owner isolation, duplicate/skip detection
- `bill_payments/tests/unpaid_by_currency_pagination.rs` — 14 comprehensive tests including archived gaps, limit clamping, owner isolation, currency case-insensitivity
- `reporting/src/tests_archived_pagination_bound.rs` — 8 tests covering bound, first-page equivalence, full traversal, out-of-range cursor, empty archive, limit normalization, user isolation
- `orchestrator/src/test.rs:432+` — audit log pagination no-duplicates across heavy execution history

---

## Reviewer Checklist

When reviewing a PR that adds or modifies a paginated read, verify all of these:

**Function signature:**
- [ ] Function has `cursor` or `from_index` parameter (not `start`/`offset`)
- [ ] Function has `limit` parameter
- [ ] Return type is a `#[contracttype]` Page struct with `items`, `next_cursor`, `count` fields (or exception: `Vec` for orchestrator audit)
- [ ] Doc comment exists and references this handbook

**Pagination logic:**
- [ ] `limit` is clamped with `clamp_limit()` (or custom clamping is justified and tested)
- [ ] Out-of-range `cursor/index` returns empty page (not panic)
- [ ] Cursor is either exclusive (ID-based) or absolute (index-based) — documented which
- [ ] `next_cursor = 0` signals final page (idempotent further calls)
- [ ] Results are ordered deterministically (ascending ID or array order)

**Storage access:**
- [ ] Function reads owner index first (if owner-scoped)
- [ ] Function respects all filters (paid/unpaid, active/deactivated, etc.)
- [ ] No full-store scans (use indexed lookups)
- [ ] Missing items in range skipped cleanly (no panic on sparse indices)

**Tests:**
- [ ] Test covers first page (cursor=0)
- [ ] Test covers middle page (cursor > 0)
- [ ] Test covers final page (fewer items returned than limit)
- [ ] Test covers out-of-range cursor (returns empty)
- [ ] Test covers limit=0 (normalized to default)
- [ ] Test covers limit > max (clamped to max)
- [ ] Test covers owner isolation (if owner-scoped)
- [ ] Test covers no duplicates/gaps across pages
- [ ] Stress test with 200+ items (ensures scaling)
- [ ] Gas benchmark test (documents cost scaling)

**Documentation:**
- [ ] Function doc comment explains cursor semantics
- [ ] If limit clamping is non-standard, explain why
- [ ] If ordering is non-deterministic, document why (and consider fixing)
- [ ] Link to related pagination docs (e.g., `docs/bill-payments-unpaid-by-currency-pagination.md` for multi-filter reads)

**Security:**
- [ ] Owner-scoped reads call `owner.require_auth()` (or auth is pushed to caller)
- [ ] No auth bypass via cursor manipulation
- [ ] Cursor isolation enforced (cursor from one owner cannot access another owner's items)

---

## Adding a New Paginated Read

Follow this template exactly when adding a new paginated function:

### Step 1: Choose cursor type

- **Exclusive ID-based cursor:** Use if index is keyed by owner/entity ID and order is stable (items only added, not reordered)
  - Storage pattern: `DataKey::OwnerIndex(owner)` → `Vec<u32>` of item IDs
  - Cursor is the ID boundary; next page starts from first ID > cursor

- **Absolute index:** Use if data is a vector and order never changes (audit logs, append-only)
  - Storage pattern: Direct `Vec<T>` in instance/persistent storage
  - Cursor is the array index; next page starts at cursor position

### Step 2: Define the Page struct

```rust
#[contracttype]
#[derive(Clone)]
pub struct MyItemPage {
    pub items: Vec<MyItem>,
    pub next_cursor: u32,
    pub count: u32,
}
```

### Step 3: Implement the function

**Exclusive ID-based (recommended pattern):**
```rust
/// Returns a page of items for the owner.
///
/// See [Pagination Handbook](../../docs/PAGINATION_HANDBOOK.md)
/// for invariants all paginated reads must satisfy.
///
/// # Parameters
/// - `owner` — account to fetch items for
/// - `cursor` — exclusive item ID boundary (0 = start)
/// - `limit` — max items to return (clamped to `MAX_PAGE_LIMIT`)
///
/// # Returns
/// Up to `limit` items ordered by ID. `next_cursor = 0` when 
/// the final page is reached. Caller resumes with 
/// `next_cursor` as the `cursor` argument on the next call.
///
/// # Panics
/// Panics if `cursor` is not 0 and not found in the owner's index
/// (stale or invalid cursor).
pub fn list_items(
    env: Env,
    owner: Address,
    cursor: u32,
    limit: u32,
) -> MyItemPage {
    let limit = clamp_limit(limit);
    
    let ids: Vec<u32> = env
        .storage()
        .persistent()
        .get(&DataKey::OwnerItems(owner.clone()))
        .unwrap_or_else(|| Vec::new(&env));
    
    if ids.is_empty() {
        return MyItemPage {
            items: Vec::new(&env),
            next_cursor: 0,
            count: 0,
        };
    }
    
    let mut start_index: u32 = 0;
    if cursor != 0 {
        if let Some(pos) = ids.iter().position(|id| id == cursor) {
            start_index = (pos as u32) + 1;
        } else {
            panic!("Invalid cursor");
        }
    }
    
    let mut end_index = start_index + limit;
    if end_index > ids.len() {
        end_index = ids.len();
    }
    
    let mut items = Vec::new(&env);
    for item_id in ids.iter().skip(start_index as usize).take(limit as usize) {
        let item = env
            .storage()
            .persistent()
            .get::<_, MyItem>(&DataKey::Item(item_id))
            .unwrap_or_else(|| panic!("Pagination index out of sync"));
        if item.owner != owner {
            panic!("Pagination index owner mismatch");
        }
        items.push_back(item);
    }
    
    let next_cursor = if end_index < ids.len() {
        ids.get(end_index - 1)
            .unwrap_or_else(|| panic!("Pagination index out of sync"))
    } else {
        0
    };
    
    MyItemPage {
        items,
        next_cursor,
        count: end_index - start_index,
    }
}
```

**Absolute index (audit log pattern):**
```rust
/// Returns a page of audit log entries (oldest-to-newest).
///
/// See [Pagination Handbook](../../docs/PAGINATION_HANDBOOK.md).
///
/// # Parameters
/// - `from_index` — zero-based array index (NOT exclusive cursor)
/// - `limit` — max entries to return (clamped to `MAX_PAGE_LIMIT`)
///
/// # Returns
/// Up to `limit` entries. `next_cursor = 0` when final page reached.
pub fn get_audit_log(env: Env, from_index: u32, limit: u32) -> AuditPage {
    let log: Option<Vec<AuditEntry>> = env
        .storage()
        .instance()
        .get(&symbol_short!("AUDIT"));
    let log = log.unwrap_or_else(|| Vec::new(&env));
    let len = log.len();
    let cap = clamp_limit(limit);
    
    if from_index >= len {
        return AuditPage {
            items: Vec::new(&env),
            next_cursor: 0,
            count: 0,
        };
    }
    
    let end = from_index.saturating_add(cap).min(len);
    let mut items = Vec::new(&env);
    for i in from_index..end {
        if let Some(entry) = log.get(i) {
            items.push_back(entry);
        }
    }
    
    let count = items.len();
    let next_cursor = if end < len { end } else { 0 };
    
    AuditPage {
        items,
        next_cursor,
        count,
    }
}
```

### Step 4: Add tests

Minimum test coverage:

```rust
#[test]
fn test_list_items_first_page() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::random(&env);
    
    // Create 5 items
    for i in 1..=5 {
        create_item(&env, &owner, i);
    }
    
    let page = list_items(&env, &owner, &0u32, &3u32);
    assert_eq!(page.items.len(), 3);
    assert_ne!(page.next_cursor, 0);
    assert_eq!(page.count, 3);
}

#[test]
fn test_list_items_out_of_range_cursor() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::random(&env);
    
    // Create 2 items
    create_item(&env, &owner, 1);
    create_item(&env, &owner, 2);
    
    let page = list_items(&env, &owner, &999_999u32, &10u32);
    assert_eq!(page.items.len(), 0);
    assert_eq!(page.next_cursor, 0);
}

#[test]
fn test_list_items_limit_clamped() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::random(&env);
    
    // Create many items
    for i in 1..=100 {
        create_item(&env, &owner, i);
    }
    
    let page = list_items(&env, &owner, &0u32, &1000u32);
    assert!(page.items.len() <= MAX_PAGE_LIMIT);
}
```

### Step 5: Document in PAGINATION_HANDBOOK.md

Add a new section to the [Paginated Entrypoints](#paginated-entrypoints) section following the template above.

### Step 6: Update README.md (if public API)

If this function is a public entrypoint, add it to the contract's README.md under the function list.

---

## Common Pitfalls

### ❌ Pitfall 1: No limit clamping

```rust
// BAD — caller can request unbounded results
pub fn get_items(env: Env, start: u32, limit: u32) -> Vec<Item> {
    for i in start..start + limit {  // limit could be u32::MAX!
        // ...
    }
}
```

### ✅ Fix: Always clamp

```rust
// GOOD
pub fn get_items(env: Env, start: u32, limit: u32) -> MyItemPage {
    let limit = clamp_limit(limit);
    // ...
}
```

### ❌ Pitfall 2: Panicking on out-of-range cursor

```rust
// BAD — caller can't safely loop to end
pub fn get_items(env: Env, cursor: u32, limit: u32) -> Vec<Item> {
    let pos = ids.iter()
        .position(|id| id == cursor)
        .expect("Cursor out of range");  // Panics!
    // ...
}
```

### ✅ Fix: Return empty page instead

```rust
// GOOD
if cursor != 0 {
    if let Some(pos) = ids.iter().position(|id| id == cursor) {
        start_index = (pos as u32) + 1;
    } else {
        return MyItemPage {
            items: Vec::new(&env),
            next_cursor: 0,
            count: 0,
        };
    }
}
```

### ❌ Pitfall 3: Inclusive cursor (wrong semantics)

```rust
// BAD — duplicates items at cursor boundary
for id in ids {
    if id >= cursor {  // INCLUSIVE — wrong!
        // process
    }
}
```

### ✅ Fix: Use exclusive cursor

```rust
// GOOD — exclusive semantics prevent duplicates
for id in ids {
    if id > cursor {  // EXCLUSIVE — correct!
        // process
    }
}
```

### ❌ Pitfall 4: Non-deterministic ordering

```rust
// BAD — map iteration order not stable
for (_, item) in items_map.iter() {
    // Order changes with map rebalancing
}
```

### ✅ Fix: Use indexed lookup with sorted index

```rust
// GOOD — stable ordering via indexed lookup
let ids: Vec<u32> = env
    .storage()
    .persistent()
    .get(&DataKey::OwnerIndex(owner))
    .unwrap_or_else(|| Vec::new(&env));  // Maintained in sorted order
    
for id in ids {
    let item = env.storage().persistent().get(&DataKey::Item(id))?;
    // ...
}
```

---

## Related Documentation

- **[pagination-limit-contract.md](pagination-limit-contract.md)** — `clamp_limit()` function contract and spec
- **[bill-payments-unpaid-by-currency-pagination.md](bill-payments-unpaid-by-currency-pagination.md)** — Detailed multi-predicate pagination case study
- **[insurance-pagination.md](insurance-pagination.md)** — Insurance policy pagination contract
- **[STORAGE_LAYOUT.md](../STORAGE_LAYOUT.md)** — Storage key design patterns
- **[docs/fw-access-audit-pagination.md](fw-access-audit-pagination.md)** — Family wallet access audit pagination
- **Stress tests:** `savings_goals/tests/stress_tests.rs` (200+ items), `bill_payments/tests/unpaid_by_currency_pagination.rs` (500+ items)
- **Benchmarks:** `savings_goals/tests/gas_bench.rs`, `bill_payments/tests/gas_bench.rs`

---

## Summary Table: All Paginated Functions

| Contract | Function | Cursor Type | Limit Cap | Return Type | File |
|----------|----------|-------------|-----------|-------------|------|
| Savings Goals | `get_goals()` | Exclusive ID | 50 | `GoalPage` | lib.rs:1622 |
| Savings Goals | `get_goals_by_tag()` | Exclusive ID | 50 | `GoalPage` | lib.rs:1695 |
| Savings Goals | `get_archived_goals_page()` | Exclusive ID | 50 | `ArchivedGoalPage` | lib.rs:1904 |
| Bill Payments | `get_unpaid_bills()` | Exclusive ID | 50 | `BillPage` | lib.rs:2153 |
| Bill Payments | `get_unpaid_bills_by_currency()` | Exclusive ID | 50 | `BillPage` | lib.rs:3286 |
| Bill Payments | `get_bills_by_currency()` | Exclusive ID | 50 | `BillPage` | lib.rs |
| Bill Payments | `get_archived_bills()` | Exclusive ID | 50 | `ArchivedBillPage` | lib.rs |
| Bill Payments | `get_archived_bills_page()` | Exclusive ID | 50 | `ArchivedBillPage` | lib.rs |
| Bill Payments | `get_bill_schedules_page()` | Exclusive ID | 50 | `BillSchedulePage` | lib.rs (Issue #1751) |
| Insurance | `get_active_policies()` | Exclusive ID | 50 | `PolicyPage` | lib.rs:742 |
| Insurance | `get_deactivated_policies()` | Exclusive ID | 50 | `PolicyPage` | lib.rs:799 |
| Remittance Split | `get_audit_log()` | Absolute Index | 50 | `AuditPage` | lib.rs:1856 |
| Remittance Split | `get_remittance_schedules_page()` | Exclusive ID | 50 | `SchedulePage` | lib.rs:2861 |
| Family Wallet | `get_member_addresses_page()` | Offset Count | Custom | `MemberAddressPage` | lib.rs:1923 |
| Reporting | `get_archived_reports_page()` | Exclusive ID | 50 | `ArchivedPage` | lib.rs:2149 |
| Orchestrator | `get_audit_log()` | Absolute Index | Custom | `Vec<AuditEntry>` | lib.rs:837 |

