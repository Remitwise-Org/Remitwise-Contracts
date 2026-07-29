# Bill Payments: `get_unpaid_bills_by_currency` Double-Predicate Pagination

**Issue:** #SC-XXX  
**Status:** ✅ Implemented  
**Test file:** `bill_payments/tests/unpaid_by_currency_pagination.rs`

---

## Overview

`get_unpaid_bills_by_currency(owner, currency, cursor, limit)` is a
**double-predicate paginated read**: it must simultaneously satisfy

1. `bill.owner == owner` — owner isolation  
2. `bill.paid == false` — unpaid filter  
3. `bill.currency == normalized(currency)` — currency filter  

This combination is where cursor bugs hide.  If the cursor advances over
filtered-out (paid) bills incorrectly, clients silently miss unpaid invoices in
a given currency — the worst possible failure for a bill-reminder UI.

---

## Pagination Contract

### Cursor semantics

- `cursor = 0` starts from the beginning of the owner's currency index.
- Each page returns `next_cursor = ID of the last returned item` (not the first
  skipped item).
- Pass `next_cursor` as the `cursor` argument on the subsequent call.
- When `next_cursor == 0`, there are no more pages.

### Ordering guarantee

Items are always returned in **strictly ascending bill ID order** across all
pages.  The currency index is maintained in ascending order so this property
holds even after paid-bill archival creates ID gaps.

### Limit clamping

`limit` is clamped by `clamp_limit()` from `remitwise-common`:

| Input `limit` | Effective limit |
|---------------|-----------------|
| `0`           | `DEFAULT_PAGE_LIMIT` (20) |
| `1–50`        | Passed through unchanged |
| `> 50`        | Clamped to `MAX_PAGE_LIMIT` (50) |

---

## Currency Normalisation

Currency strings are normalised before index lookup via
`normalize_currency()`:

1. Trim leading/trailing ASCII spaces  
2. Convert to uppercase  
3. Empty string → `"XLM"`  

**Examples:** `"usdc"` → `"USDC"`, `" Xlm "` → `"XLM"`, `""` → `"XLM"`

Normalisation is applied to **both** the stored bill and the query argument,
so queries are always case-insensitive.

---

## Implementation Detail: Currency Index

`get_unpaid_bills_by_currency` traverses the per-`(owner, currency)` index
(`STORAGE_CURRENCY_INDEX`) rather than the full owner index.  This makes the
traversal O(bills_in_currency) rather than O(all_owner_bills).

The `paid` filter is applied on each fetched bill *after* the cursor check, so
paid gaps inside the currency index do **not** cause the cursor to skip
qualifying unpaid bills.  The staging buffer holds up to `limit + 1` items to
detect whether a next page exists.

### Archived-bill gaps

When paid bills are archived (`archive_paid_bills`), they are removed from
both the active bill map and the currency index.  Subsequent pagination calls
skip the now-missing IDs cleanly because:

```
let Some(bill) = bills.get(id) else { continue; };
```

Missing entries are skipped without advancing the cursor incorrectly.

---

## Test Coverage

All tests live in `bill_payments/tests/unpaid_by_currency_pagination.rs`.

| Test name | Scale | What it verifies |
|-----------|-------|------------------|
| `union_equals_set_n50` | N=50 | No misses, no dupes across all pages |
| `union_equals_set_n200` | N=200 | Same at medium scale |
| `union_equals_set_n1000` | N=500+ active | Same at large scale |
| `cursor_monotonicity` | 30 bills | `next_cursor` strictly increases, loop terminates |
| `limit_clamped_to_max_page_limit` | 80 bills | limit=200 → at most 50 items/page |
| `limit_zero_uses_default` | 30 bills | limit=0 → 20 items/page |
| `owner_isolation` | 10+5 bills | Other owner's bills never appear |
| `zero_unpaid_in_currency` | 5 paid bills | Empty result, next_cursor=0 |
| `all_bills_one_currency` | 25 bills | Full set returned when all bills share currency |
| `cursor_past_end_returns_empty` | 5 bills | cursor=999_999 → empty page |
| `archived_gaps_do_not_cause_misses` | 30 bills | Archive-created gaps skipped cleanly |
| `multi_currency_no_bleed` | 15 USDC + 15 XLM | Currencies do not bleed into each other |
| `currency_query_case_insensitive` | 5 bills | "usdc"/"Usdc" == "USDC" |
| `result_order_strictly_ascending` | 20 bills | IDs strictly ascending across pages |

### Running the tests

```bash
cargo test -p bill_payments --test unpaid_by_currency_pagination -- --nocapture
```

Or with the full bill_payments suite:

```bash
cargo test -p bill_payments -- --nocapture
```

---

## Security Notes

- **Owner isolation is enforced at the index level**: the currency index is
  keyed by `(Address, currency)`, so one owner's query physically cannot
  traverse another owner's index entries.
- **No auth bypass**: the function calls `owner.require_auth()` implicitly via
  the Soroban SDK client layer.
- **Limit clamping prevents DoS**: a caller cannot request an unbounded page
  size; the maximum response is always ≤ 50 items.

---

## Related Documents

- [Bill Payments: Overflow-Safe Aggregation](bill-payments-aggregation.md)
- [Pagination Gaps Tests](../bill_payments/tests/pagination_gaps.rs)
- [Storage Layout](../STORAGE_LAYOUT.md)
