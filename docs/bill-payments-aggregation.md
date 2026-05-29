# Bill Payments: Overflow-Safe Aggregation

**Issue:** #627  
**Status:** ✅ Resolved  
**Date:** 2026-05-28

## Overview

The `get_total_unpaid()` and `get_total_unpaid_by_currency()` functions aggregate unpaid bill amounts across all bills for an owner. With many large-amount bills, the sum can exceed i128::MAX. This document specifies the overflow-safe aggregation policy and test coverage.

## Aggregation Functions

### `get_total_unpaid(owner: Address) -> i128`

**Location:** `bill_payments/src/lib.rs:2106`

**Purpose:** Sum all unpaid bill amounts for the given owner, regardless of currency.

**Signature:**
```rust
pub fn get_total_unpaid(env: Env, owner: Address) -> i128
```

**Algorithm:**
1. Check cache (unpaid-totals map) for fast path
2. Iterate all bills in storage
3. For each bill: if `!bill.paid && bill.owner == owner`, accumulate with **saturating addition**
4. Return result (or i128::MAX if overflow would occur)

### `get_total_unpaid_by_currency(owner: Address, currency: String) -> i128`

**Location:** `bill_payments/src/lib.rs:2293`

**Purpose:** Sum all unpaid bill amounts for the owner in a specific currency.

**Signature:**
```rust
pub fn get_total_unpaid_by_currency(env: Env, owner: Address, currency: String) -> i128
```

**Algorithm:**
1. Normalize currency (case-insensitive, trim whitespace)
2. Iterate all bills
3. For each bill: if `!bill.paid && bill.owner == owner && bill.currency == normalized`, accumulate with **saturating addition**
4. Return result (or i128::MAX if overflow would occur)

---

## Overflow Policy: Saturating Addition

### Rationale

With many bills approaching i128::MAX / 2, the total can overflow. To prevent panic and maintain predictable semantics, we use **saturating addition**:

```rust
total = total.saturating_add(bill.amount);
```

**Behavior:**
- **Normal (no overflow):** `result = a + b`
- **Overflow (sum > i128::MAX):** `result = i128::MAX`

### Why Saturating?

1. **Incident Resilience** — Aggregation functions cannot panic; the contract must remain responsive.
2. **Upper-Bound Guarantee** — The result is always ≤ i128::MAX, predictable for consumers.
3. **Audit Trail** — A saturated total signals "very large balance"; consumers can investigate.
4. **No Silent Wrap-Around** — Unlike unchecked add (which wraps to negative), saturation makes overflow visible.

### Trade-Off

**Limitation:** A saturated total of i128::MAX indicates "at least this much;" the exact value is unknown.

**Mitigation:** Operators should implement alerts if `get_total_unpaid() == i128::MAX` to trigger manual review.

---

## Currency Filtering

### Normalization

Currency strings are normalized to enable case-insensitive filtering:

```rust
let normalized_currency = Self::normalize_currency(&env, &currency);
```

**Normalization Rules:**
1. Trim leading/trailing whitespace
2. Convert to uppercase
3. Default empty string to "XLM"

**Examples:**
- `"usdc"` → `"USDC"`
- `" XLM "` → `"XLM"`
- `""` → `"XLM"`
- `"USDC"` → `"USDC"`

### Aggregation Correctness

`get_total_unpaid_by_currency()` filters by exact normalized currency. No cross-currency mixing occurs.

**Test:** `test_get_total_unpaid_by_currency_saturates_on_overflow()`

---

## Test Coverage

### 1. Near-Maximum Totals

**Test:** `test_get_total_unpaid_with_two_large_bills()`

Creates two bills at i128::MAX / 2 each and verifies `get_total_unpaid()` returns their saturated sum.

### 2. Overflow Saturation (get_total_unpaid)

**Test:** `test_get_total_unpaid_saturates_on_overflow()`

**Scenario:**
- Create Bill1 with amount = i128::MAX / 2 + 1000
- Create Bill2 with amount = i128::MAX / 2 + 1000
- Call `get_total_unpaid(owner)`

**Expected Result:**
- Returns i128::MAX (saturated)
- ✅ Does NOT panic

**Previous Behavior (Issue):** Would panic on overflow ❌

### 3. Overflow Saturation (get_total_unpaid_by_currency)

**Test:** `test_get_total_unpaid_by_currency_saturates_on_overflow()`

**Scenario:**
- Create two USDC bills that overflow
- Create one XLM bill (1000)
- Query both currencies

**Expected Results:**
- `get_total_unpaid_by_currency(owner, "USDC")` → i128::MAX (saturated)
- `get_total_unpaid_by_currency(owner, "XLM")` → 1000 (only XLM bill)
- ✅ Currency filtering works correctly
- ✅ No cross-currency overflow

### 4. Empty Owner

**Implicit Test:** `test_get_total_unpaid(owner_with_no_bills)` → 0

**Expected:** Returns 0 (no bills to sum)

### 5. Multiple Owners (Isolation)

**Test:** `test_multiple_large_bills_different_owners()`

**Scenario:**
- Owner1 has large bills
- Owner2 has large bills
- Each owner's total is computed independently

**Expected:** Each `get_total_unpaid(owner)` aggregates only that owner's bills.

---

## Performance

### Caching Strategy

`get_total_unpaid()` leverages a cached unpaid-totals map for O(1) retrieval on repeated queries:

```rust
if let Some(totals) = Self::get_unpaid_totals_map(&env) {
    if let Some(total) = totals.get(owner.clone()) {
        return total;  // Fast path
    }
}
```

**Cache Invalidation:**
- Cache is cleared on bill creation
- Cache is cleared on bill payment
- Cache is cleared on bill cancellation

**Note:** `get_total_unpaid_by_currency()` does NOT use cache (per-currency totals not cached). This may be optimized in future work.

---

## Implementation Details

### Saturating Addition in Code

**bill_payments/src/lib.rs:2119-2120 (get_total_unpaid)**
```rust
for (_, bill) in bills.iter() {
    if !bill.paid && bill.owner == owner {
        total = total.saturating_add(bill.amount);  // ← SATURATING
    }
}
```

**bill_payments/src/lib.rs:2305-2307 (get_total_unpaid_by_currency)**
```rust
for (_, bill) in bills.iter() {
    if !bill.paid && bill.owner == owner && bill.currency == normalized_currency {
        total = total.saturating_add(bill.amount);  // ← SATURATING
    }
}
```

---

## Breaking Changes

**Before:** Overflow would panic; transaction reverted.  
**After:** Overflow saturates; returns i128::MAX; transaction continues.

### Consumer Impact

1. **Test Updates:** Any test expecting panic must be updated (see `test_get_total_unpaid_saturates_on_overflow()`)
2. **Monitoring:** Operators should monitor for `get_total_unpaid() == i128::MAX` and investigate
3. **Semantics:** Code must handle i128::MAX as "very large balance," not a special error code

---

## Related Issues

- GitHub Issue: #627
- Linked PR: (see PR description for number)

---

## Changelog

- **2026-05-28:** Implemented saturating addition; updated tests; added currency-filtering tests.

---

## References

- [Saturating Arithmetic (Rust Docs)](https://doc.rust-lang.org/std/primitive.i128.html#method.saturating_add)
- Bill struct definition: `bill_payments/src/lib.rs`
- Test module: `bill_payments/tests/stress_test_large_amounts.rs`
