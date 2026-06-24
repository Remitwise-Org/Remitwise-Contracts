# PR Description — Issue #832: Bound `get_archived_reports`

## Summary

Closes #832.

This PR implements the security/perf fix for the unbounded `get_archived_reports`
reader in the `reporting` contract. The reader now returns at most
`DEFAULT_PAGE_LIMIT` (20) entries (closing the latent host-budget DoS) and is
formally deprecated in favor of the already-paginated
`get_archived_reports_page` reader, which now follows the canonical terminator
convention (`next_cursor == 0`).

### Behaviour changes

- `get_archived_reports(env, user)` now delegates to
  `get_archived_reports_page(user, 0, DEFAULT_PAGE_LIMIT)` and returns at most
  the first `DEFAULT_PAGE_LIMIT` (20) entries. The signature is **preserved**
  for back-compat but the function is marked `#[deprecated]`.
- `get_archived_reports_page(env, user, cursor, limit)`:
  - Out-of-range cursors (`cursor >= count`) and empty archives now return
    `next_cursor == 0` (canonical terminator) instead of echoing the cursor
    back.
  - `limit` is normalized via `remitwise_common::clamp_limit`: `0` →
    `DEFAULT_PAGE_LIMIT` (20); values above `MAX_PAGE_LIMIT` (50) are clamped
    to `MAX_PAGE_LIMIT`.
  - Cursor termination is now guaranteed across all inputs (in-range,
    out-of-range, empty archive, oversized limit).

### Files changed

| File | Change |
|---|---|
| `reporting/src/lib.rs` | Imported `DEFAULT_PAGE_LIMIT`; marked `get_archived_reports` `#[deprecated]` and delegated to the paged reader; tightened `get_archived_reports_page` to use the canonical terminator and `clamp_limit` normalization; updated doc comments. |
| `reporting/src/tests_archived_pagination_bound.rs` | New module. 8 tests covering bound enforcement, first-page equivalence (deprecated vs paged), full archival traversal, out-of-range cursor, empty archive, `limit=0` normalization, `limit=u32::MAX` clamping, and user isolation under bound. |
| `CHANGELOG_CONTRACTS.md` | New `## Reporting → ### v0.2.0` entry above the existing `v0.1.0`. Documents the bound, deprecation, terminator convention, migration, and `#832` link. |
| `reporting/README.md` | Replaced the `get_archived_reports` row under **Admin Maintenance** with `get_archived_reports_page` including pagination contract and a **`get_archived_reports` deprecation pointer (`Issue #832`)** pointing back at the paged API. The deprecated entry remains in the **Authorization Model** table for grep discoverability. |

### Acceptance criteria

| Requirement | Status |
|---|---|
| `get_archived_reports` no longer unbounded | ✅ capped at `DEFAULT_PAGE_LIMIT` (20) via delegation to the paged reader |
| Paged reader verified terminating + non-panicking | ✅ `tests_archived_pagination_bound.rs::paged_reader_walks_entire_archive_and_terminates`, `::paged_reader_out_of_range_cursor_returns_empty_page_with_terminator`, `::paged_reader_empty_archive_returns_terminator` |
| Deprecation noted in changelog + docs | ✅ `CHANGELOG_CONTRACTS.md` v0.2.0 + `reporting/README.md` deprecation note |
| Test coverage | ✅ 8 new tests in `reporting/src/tests_archived_pagination_bound.rs` exercising the bound terminator, normalization, equivalence, and user isolation |
| `cargo test -p reporting` + clippy clean | Required: re-run on a host with `cargo` installed |

### Migration guidance for integrators

Replace calls to `get_archived_reports(user)` with the canonical paged walk:

```rust
let mut cursor = 0u32;
loop {
    let page = client.get_archived_reports_page(&user, &cursor, &DEFAULT_PAGE_LIMIT);
    // ... process page.items ...
    if page.next_cursor == 0 { break; }
    cursor = page.next_cursor;
}
```

No storage migration is required. The signature of the deprecated reader is
**unchanged**, so existing callers that only inspect the first page (≤ 20
entries) keep working without code changes.

### Implementation notes

The bound is implemented by **delegation**, not by duplicating the loop logic.
This guarantees a single source of truth for the cursor/limit/index walk and
removes any drift risk between the two readers. The paged reader's `limit`
is normalized via `remitwise_common::clamp_limit` to match every other
paginated read in the Remitwise suite (`docs/pagination-limit-contract.md`).

### Verification commands

```bash
cargo test -p reporting
cargo clippy -p reporting --no-deps --all-targets -- -D warnings
cargo fmt --check
```

> **Note:** the `deny(clippy::unwrap_used)` and `deny(clippy::expect_used)`
> attributes in `lib.rs` apply only outside `#[cfg(test)]`, so the affected
> legacy callers in `tests.rs` / `tests_updated.rs` / `tests_auth_acl.rs`
> produce only **warnings** (not errors) when they call the now-deprecated
> `get_archived_reports`. Tests still pass without `#[allow(deprecated)]`,
> but those warnings can be silenced in a follow-up cleanup if desired.

## Linked issue

Closes #832
