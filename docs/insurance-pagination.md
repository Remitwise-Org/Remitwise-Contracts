# Insurance Pagination Contract

This document defines the deterministic paging contract for `get_active_policies(owner, cursor, limit)`.

## Contract

- Return type: `PolicyPage { items, next_cursor, count }`.
- Ordering: `items` are sorted by ascending policy id.
- Scope: only active policies belonging to `owner`.
- Limit behavior:
- `limit == 0` uses `DEFAULT_PAGE_LIMIT`.
- `limit > MAX_PAGE_LIMIT` is clamped to `MAX_PAGE_LIMIT`.
- Cursor behavior:
- `cursor` is exclusive (`id > cursor`).
- `next_cursor` equals the last returned id only when another page exists.
- `next_cursor == 0` means terminal page (no further active policies).

## Determinism And Safety

- Determinism is enforced by sorting owner-indexed ids before paging.
- Sparse ids (from deactivation/archival) do not break ordering or traversal.
- Paging reads are bounded to the owner index (`KEY_OWNER_INDEX` / active checks via `KEY_OWNER_ACTIVE` counters and policy state), avoiding full-map scans.
