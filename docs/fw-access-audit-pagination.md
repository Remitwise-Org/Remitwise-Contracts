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
