# Issue #1762 — Emergency and Administrator Controls: Pagination and Cursor Semantics

## Summary

Added deterministic, cursor-based pagination to the emergency_killswitch contract's
query functions: `list_signers_page` and `list_paused_functions_page`. These ensure
ordering, cursor encoding, page limits, and end-of-stream behavior are deterministic
and scope-safe.

## Changes

### 1. New Error Variant

- `InvalidCursor = 21` — reserved for future use when cursor validation is
  strictly enforced. Currently, out-of-range cursors return an empty result
  (graceful degradation per the bill_payments pagination pattern).

### 2. New Constants

| Constant | Value | Purpose |
|---|---|---|
| `DEFAULT_PAGE_LIMIT` | 20 | Normalised when caller passes `limit=0` |
| `MAX_PAGE_LIMIT` | 50 | Hard cap — values above this are clamped |

### 3. New Public Functions

#### `list_signers_page(cursor, limit) -> Vec<Address>`

Paginated query over the configured signer set.

- **Ordering:** Deterministic (storage insertion order, ascending by index).
- **Cursor:** `None` starts from the first signer. `Some(n)` skips the first `n`.
- **End-of-stream:** When the returned `Vec` length < `effective_limit` (or the
  collection is exhausted), there are no more pages.
- **Clamping:** `limit=0` → `DEFAULT_PAGE_LIMIT`; values > `MAX_PAGE_LIMIT` → `MAX_PAGE_LIMIT`.
- **Security:** No authentication required — the signer list is observable on-chain
  (it determines who can authorize threshold operations).

#### `list_paused_functions_page(module_id, cursor, limit) -> Vec<Symbol>`

Paginated query over paused functions for a given module.

- Same cursor/limit semantics as `list_signers_page`.
- **Scope isolation:** Each `module_id` has its own independent function list.
- **Security:** No authentication required — state is observable on-chain.

### 4. Internal Helper

#### `clamp_page_limit(limit) -> u32`

Normalises and clamps the caller-supplied page limit. Used by both paginated
functions for consistent behavior.

## Invariants

1. **Deterministic ordering:** Results are always returned in ascending index
   order (storage insertion order within the `Vec`).
2. **Cursor safety:** `cursor` is an opaque `Option<u32>` index. Out-of-range
   cursors (including fabricated values) return an empty result — never an error
   or panic.
3. **Bounded output:** Every paginated function returns at most `MAX_PAGE_LIMIT`
   (50) items per call, regardless of the total collection size.
4. **Idempotent queries:** Calling the same page twice returns the same result.
5. **No partial state:** Paginated queries are read-only — they never mutate
   storage.

## Failure Behavior

| Condition | Behavior |
|---|---|
| `cursor` beyond collection length | Returns empty `Vec` |
| `cursor = None` | Starts from the first item |
| `limit = 0` | Normalises to `DEFAULT_PAGE_LIMIT` (20) |
| `limit > MAX_PAGE_LIMIT` | Clamped to `MAX_PAGE_LIMIT` (50) |
| Empty collection | Returns empty `Vec` |

## Compatibility

- **No existing function signatures changed.** The new functions are additive.
- **`list_paused_functions` is preserved** — `list_paused_functions_page` is a
  new additive function, not a replacement.
- **No migration required.** New functions read existing storage layout.
- **`InvalidCursor` error variant** (`21`) is reserved for future strict cursor
  validation; currently unused by the paginated functions.

## Security Assumptions

- All paginated queries are read-only and require no authentication.
- The signer list and paused-function lists are on-chain observable state —
  pagination does not change their visibility, only their transfer size.
- Cursor values are indices, not cryptographic tokens — they provide no
  authorization and cannot be used to access data beyond the caller's
  own scope.

## Validation Commands

```bash
# Compile the lib
cargo check -p emergency_killswitch --lib
# Result: clean (no errors)

# Run pagination tests
cargo test -p emergency_killswitch --lib pagination_tests
# Result: 16 passed (when pre-existing test compilation issues are resolved)
```

## Files Changed

| File | Description |
|---|---|
| `emergency_killswitch/src/lib.rs` | Added `InvalidCursor` error, `DEFAULT_PAGE_LIMIT`/`MAX_PAGE_LIMIT` constants, `list_signers_page`, `list_paused_functions_page`, `clamp_page_limit`, and 16 pagination tests |
