# Remittance Split Rounding & Dust Policy

## Overview

`calculate_split_amounts` divides a total USDC amount across four categories —
**spending**, **savings**, **bills**, and **insurance** — according to owner-configured
percentages that must sum to exactly 100.

Because the contract runs on Stellar/Soroban and stores amounts as `i128` integers,
dividing by 100 produces floor-truncated results. The fractional portions discarded
by truncation (collectively called **dust** or the **remainder**) must be assigned
somewhere to preserve the conservation invariant `sum(allocations) == total_amount`.

## Algorithm

Given `total_amount` (must be > 0) and configured percentages
`spending_pct`, `savings_pct`, `bills_pct`, `insurance_pct` (summing to 100):

```
spending  = floor(total_amount × spending_pct  / 100)
savings   = floor(total_amount × savings_pct   / 100)
bills     = floor(total_amount × bills_pct     / 100)
insurance = total_amount − spending − savings − bills
```

Each of spending, savings, and bills is an independent `checked_mul` followed by
`checked_div(100)`. Insurance is computed last via three `checked_sub` calls on
`total_amount`, so it naturally absorbs whatever is left over.

## Dust Assignment Rule

**Insurance always receives the remainder.**

Formally:

```
remainder = total_amount − floor(total_amount × spending_pct / 100)
                         − floor(total_amount × savings_pct  / 100)
                         − floor(total_amount × bills_pct    / 100)

insurance_allocation = remainder
```

This means `insurance_allocation ≥ floor(total_amount × insurance_pct / 100)`.
The excess above the floor is the dust. The dust is deterministic: given identical
inputs it is always the same value and always lands in insurance.

Insurance was chosen as the dust recipient because it is the fourth and final
category; its allocation is derived by subtraction rather than multiplication,
making the conservation property algebraically guaranteed without a separate
correction step.

## Conservation Guarantee

**Post-condition (formally):**

```
spending_allocation + savings_allocation + bills_allocation + insurance_allocation
  == total_amount
```

This holds for every valid input because insurance is defined as:

```
insurance = total_amount − spending − savings − bills
```

Substituting back:

```
spending + savings + bills + (total_amount − spending − savings − bills)
  = total_amount  ✓
```

No rounding correction is needed; the identity is structural.

## Batch / fan-out conservation across schedule sweeps

`execute_due_remittance_schedules()` sweeps _many_ `RemittanceSchedule` entries in one call and advances each schedule’s `next_due`/`last_executed`.

Even though the per-schedule dust assignment is structurally conservative, a bug can still leak/destroy value at scale if the sweep logic is wrong (e.g., double-executing the same schedule window, skipping a due schedule, or advancing `next_due` incorrectly).

### Batch conservation invariant

For any ledger timestamp `T`, let `S(T)` be the exact set of schedules with:

- `active == true`, and
- `next_due <= T` (idempotency-guarded by `last_executed`),

and let `amount_i` be the scheduled `amount` for each `s_i ∈ S(T)`.

Then the total distributed allocations across the entire fan-out must satisfy:

```
Σ_{s_i in S(T)} amount_i
  ==
Σ_{s_i in S(T)} (spending_i + savings_i + bills_i + insurance_i)
```

Because dust is always deterministically assigned to `insurance` inside `calculate_split`, proving this aggregate equality pins the dust policy across the whole sweep — not just per-call.

### Why idempotency matters

The executor advances `last_executed = T` before moving `next_due`. This prevents re-executing the same schedule in the same due window, which would otherwise cause aggregate double-counting (and therefore aggregate value leaks).

## Overflow Protection

Each of the three multiplication steps uses Rust's `checked_mul`, and each
subtraction uses `checked_sub`. If any step would overflow or underflow `i128`,
the function returns `Err(RemittanceSplitError::Overflow)` immediately, **before**
any partial allocation value is produced or stored.

Overflow can occur when `total_amount × pct` exceeds `i128::MAX` (~1.7 × 10³⁸).
For reference, `i128::MAX × 2` already overflows, so amounts above
`i128::MAX / max_pct` trigger this guard.

## Test Coverage Table

| Test         | Amount              | Percentages (sp/sv/bl/ins) | What it proves                                       |
| ------------ | ------------------- | -------------------------- | ---------------------------------------------------- |
| conservation | 1                   | 25/25/25/25                | Equal split, small amount                            |
| conservation | 1                   | 50/50/0/0                  | Zero-pct categories, dust in insurance               |
| conservation | 3                   | 33/33/33/1                 | Non-divisible amount, remainder = 0                  |
| conservation | 7 (odd prime)       | 40/30/20/10                | Unequal split, odd prime amount                      |
| conservation | 11 (odd prime)      | 34/33/33/0                 | Zero insurance pct, remainder goes to insurance      |
| conservation | 97 (odd prime)      | 33/33/33/1                 | Large remainder on odd prime                         |
| conservation | 100                 | 50/50/0/0                  | Even split, zero remainder                           |
| conservation | 999                 | 33/33/33/1                 | 3-digit near-1000, remainder visible                 |
| conservation | i128::MAX/1_000_000 | 25/25/25/25                | Large amount, no overflow                            |
| conservation | i128::MAX/1_000_000 | 33/33/33/1                 | Large amount + remainder                             |
| conservation | i128::MAX/1_000_000 | 40/30/20/10                | Large amount, unequal split                          |
| isolation    | 10                  | 33/33/33/1                 | Exact floor values + remainder verified per-category |
| overflow     | i128::MAX           | 50/50/0/0                  | Intermediate product overflows → Overflow error      |
| zero         | 0                   | 25/25/25/25                | amount ≤ 0 → InvalidAmount before any allocation     |

## Worked Example

**Input:** `total_amount = 10`, percentages = `33 / 33 / 33 / 1`

```
spending  = floor(10 × 33 / 100) = floor(3.3)  = 3
savings   = floor(10 × 33 / 100) = floor(3.3)  = 3
bills     = floor(10 × 33 / 100) = floor(3.3)  = 3
insurance = 10 − 3 − 3 − 3 = 1
```

Conservation check: `3 + 3 + 3 + 1 = 10 ✓`

The insurance percentage is 1%, so its floor share would be `floor(10 × 1 / 100) = 0`.
The remainder is `1 − 0 = 1`, and it is added entirely to insurance, bringing it to 1.
