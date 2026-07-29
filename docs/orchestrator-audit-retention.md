# Orchestrator Audit Retention

## Overview

The orchestrator contract maintains a bounded audit log of every flow execution and privileged action. The log is stored under the `AUDIT` instance-storage key as a `Vec<AuditEntry>` and is capped at `MAX_AUDIT_ENTRIES = 100` records.

## Ring-buffer eviction

When `append_audit` is called and the log has reached `MAX_AUDIT_ENTRIES`, the **oldest entry is evicted** before the new one is appended. This keeps instance-storage size and read cost constant regardless of how many executions have occurred.

Each eviction increments `ExecutionStats::evicted_entries` (stored under `STATS`). Operators can read this counter via `get_execution_stats()` to detect that rotation has occurred without reading the full log.

## Pagination

`get_audit_log(from_index, limit)` returns a slice of the current bounded window:

| Parameter | Behaviour |
|-----------|-----------|
| `from_index` | Zero-based cursor into the current window (oldest entry = 0). Out-of-range → empty page. |
| `limit = 0` | Defaults to 20 entries. |
| `limit > MAX_AUDIT_ENTRIES` | Clamped to `MAX_AUDIT_ENTRIES` (100). |

Page-end calculation uses `saturating_add` to prevent cursor overflow.

### Example — reading all entries in pages of 25

```rust
let mut from = 0u32;
loop {
    let page = client.get_audit_log(&from, &25);
    if page.is_empty() { break; }
    process(page);
    from += 25;
}
```

## Entry ordering

Entries are ordered **oldest-to-newest** within the current window. After rotation, index 0 is the oldest *surviving* entry, not the globally oldest entry ever written.

## External archival

Because the log rotates, clients that need long-term retention must read and persist entries externally before they are evicted. The recommended pattern is:

1. Poll `get_execution_stats()` and compare `evicted_entries` against a locally stored baseline.
2. If the counter has advanced, read the full current window and archive any entries not yet stored.
3. Update the local baseline.

## Storage cost

With `MAX_AUDIT_ENTRIES = 100` and each `AuditEntry` occupying roughly 80 bytes (Symbol + Address + u64 + bool), the `AUDIT` key consumes at most ~8 KB of instance storage, keeping rent predictable.

## Constants

| Constant | Value | Location |
|----------|-------|----------|
| `MAX_AUDIT_ENTRIES` | 100 | `orchestrator/src/lib.rs` |
| Default page size | 20 | `clamp_limit()` in `orchestrator/src/lib.rs` |
