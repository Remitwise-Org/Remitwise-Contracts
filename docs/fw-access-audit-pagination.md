# Family Wallet — Access-Audit Pagination

## Overview

`get_access_audit_page` exposes the `ACC_AUDIT` log to Owner/Admin callers in
fixed-size pages.  This document describes the cursor contract, clamping rules,
and iteration protocol that off-chain consumers must follow.

---

## Storage key

| Key | Type | Description |
|-----|------|-------------|
| `ACC_AUDIT` | `Vec<AccessAuditEntry>` | Rolling audit trail, capped at `MAX_ACCESS_AUDIT_ENTRIES` (200). Oldest entries are evicted when the cap is reached. |

---

## Types

```rust
pub struct AccessAuditEntry {
    pub operation: Symbol,   // short label, e.g. "em_mode", "add_mem"
    pub caller:    Address,
    pub target:    Option<Address>,
    pub success:   bool,
    pub timestamp: u64,      // ledger timestamp at write time
}

pub struct AccessAuditPage {
    pub items:       Vec<AccessAuditEntry>,
    pub next_cursor: u32,  // pass as `from_index` on the next call
    pub count:       u32,  // number of items in this page
}
```

---

## Cursor semantics

`from_index` is the **inclusive, zero-based** index of the first entry to
return.  `next_cursor` in the response is the index to supply as `from_index`
on the next call.

### End-of-log sentinel

`next_cursor == total` (where `total` is the length of the log at call time)
signals that there are no more entries.  Callers should stop iterating when:

- `next_cursor >= total` (returned by a previous page), **or**
- the returned page is empty (`count == 0`).

> **Why not `0`?**  Index `0` is a valid start position.  Using `0` as a
> "done" sentinel would force callers to special-case the first page and would
> cause an infinite loop if naively re-submitted.  Using `total` is
> unambiguous: it is always one past the last valid index.

---

## Clamping rules (no panic on adversarial input)

| Input condition | Behaviour |
|-----------------|-----------|
| `limit == 0` | Promoted to `DEFAULT_AUDIT_PAGE_LIMIT` (20) |
| `limit > MAX_AUDIT_PAGE_LIMIT` (50) | Clamped to `MAX_AUDIT_PAGE_LIMIT` |
| `from_index >= total` | Returns empty page; `next_cursor = total` |
| `from_index == u32::MAX` | Handled by the `>= total` check; no overflow |

---

## Ring-buffer eviction (bounded trail)

`ACC_AUDIT` is a **bounded ring buffer**, not an unbounded log. Each privileged
operation appends one entry via `append_access_audit`. When the length exceeds
`MAX_ACCESS_AUDIT_ENTRIES` (200), the **oldest** entries are evicted so that only
the most-recent 200 are retained:

- After appending entry number `n` where `n > 200`, entries with insertion
  positions `0 .. n-200` are dropped and the retained set is **re-indexed from
  `0`**. The surviving entries keep their relative order (oldest-retained first,
  newest last).
- Pagination operates over these **post-eviction indices**. `from_index = 0`
  always points at the oldest *retained* entry — never at an evicted one — and
  `total` (revealed by the end-of-log sentinel) is capped at `200`.
- Consequently, an evicted entry can never reappear on any page, and the cursor
  stays strictly monotonic across the wrap boundary (no skips, no duplicates).

> **Forensic implication:** the trail is a *rolling window*. Once 200 newer
> privileged operations have occurred, evidence of the oldest ones is gone.
> Off-chain consumers that need a permanent record must paginate and persist the
> trail *before* it wraps. The cap bounds instance-storage rent; it is not a
> retention guarantee.

### Worked example

Seed 250 operations with ascending timestamps `1000 .. 1249`:

| | Insertion positions | Timestamps | State |
|---|---|---|---|
| Evicted | `0 .. 49` | `1000 .. 1049` | dropped |
| Retained | `50 .. 249` | `1050 .. 1249` | re-indexed to page positions `0 .. 199` |

`get_access_audit_page(caller, 0, 50)` returns the entry with timestamp `1050`
first; the end-of-log sentinel `next_cursor` reaches `200`.

---

## Relationship to `get_access_audit`

`get_access_audit(limit)` returns the **tail** — the newest
`min(limit, total)` retained entries, oldest-to-newest. This is exactly the
**suffix** of the fully-paginated view (paging from index `0` to the sentinel).
The two read paths are consistent by construction, including across an eviction
wrap: paging the whole log and taking its last `k` entries yields the same
entries, in the same order, as `get_access_audit(k)`.

---

## Iteration example (off-chain pseudo-code)

```typescript
let cursor = 0;
loop {
    const page = await contract.get_access_audit_page(caller, cursor, 20);
    process(page.items);
    if (page.count === 0 || page.next_cursor >= knownTotal) break;
    cursor = page.next_cursor;
}
```

Because `next_cursor` is always `i` after the last consumed entry, and `i`
equals `total` when the log is exhausted, the loop terminates without
skipping or duplicating entries even if new entries are appended between calls.

---

## Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_ACCESS_AUDIT_ENTRIES` | 200 | Rolling cap on stored entries |
| `MAX_AUDIT_PAGE_LIMIT` | 50 | Maximum entries per page |
| `DEFAULT_AUDIT_PAGE_LIMIT` | 20 | Page size when `limit == 0` |

---

## Access control

`get_access_audit_page` requires the caller to hold at least the `Admin` role
(Owner or Admin).  Regular `Member` addresses are rejected.

`get_access_audit` (the simpler tail-read variant) has no role check and
returns the last `min(limit, log_length)` entries.

---

## Test coverage

Tests live in `family_wallet/src/test.rs` under the
`// Access-Audit Pagination Tests` section:

| Test | What it verifies |
|------|-----------------|
| `test_audit_page_empty_log_returns_sentinel` | Empty log → `count=0`, `next_cursor=0` (sentinel) |
| `test_audit_page_offset_beyond_length_returns_sentinel` | `from_index=100` on 3-entry log → empty page, `next_cursor=3` |
| `test_audit_page_offset_u32_max_no_panic` | `from_index=u32::MAX` → no panic, correct sentinel |
| `test_audit_page_limit_zero_uses_default` | `limit=0` → `DEFAULT_AUDIT_PAGE_LIMIT` entries returned |
| `test_audit_page_oversized_limit_clamped_to_max` | `limit=u32::MAX` → clamped to `MAX_AUDIT_PAGE_LIMIT` |
| `test_audit_page_limit_larger_than_remaining_returns_tail` | Partial last page returns only remaining entries |
| `test_audit_page_exact_boundary_last_entry` | Reading the very last entry yields correct sentinel |
| `test_audit_page_single_entry_log` | Single-entry log; second call with returned cursor is empty |
| `test_audit_page_full_iteration_no_skip_no_duplicate` | Full iteration collects every entry exactly once |
| `test_audit_page_cursor_stable_across_calls` | Same cursor returns identical results on repeated calls |

Ring-eviction, authorization, and cross-read tests live under the
`// Access-Audit Ring-Eviction & Cross-Read Tests` section:

| Test | What it verifies |
|------|-----------------|
| `test_audit_ring_evicts_oldest_retains_newest` | Past the cap, oldest evicted / newest retained; total pinned at 200; evicted prefix never paged |
| `test_audit_ring_exactly_at_max_evicts_nothing` | Exactly 200 entries → nothing evicted, oldest still present |
| `test_audit_ring_one_over_max_evicts_single_oldest` | 201 entries → exactly the single oldest evicted |
| `test_audit_page_post_eviction_no_skip_no_duplicate` | Paging post-eviction with a non-dividing page size is contiguous and monotonic |
| `test_audit_page_authorization_matches_policy` | Owner/Admin allowed; `Member` and non-member rejected by the role gate |
| `test_get_access_audit_consistent_with_paginated_view` | `get_access_audit(limit)` equals the tail of the paginated view, across an eviction wrap |
