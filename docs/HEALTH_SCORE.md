# Financial Health Score

This document specifies the algorithm behind `HealthScore`, the headline number
produced by the `reporting` contract. It is the authoritative reference for
reviewers auditing the math and for integrators explaining the number to end
users.

> **Source of truth:** `reporting/src/lib.rs` — `ReportingContract::calculate_health_score`
> and the private helpers `calculate_savings_score`, `calculate_bills_score`,
> `calculate_insurance_score`, and `clamp_score`. This document is written to
> match that code exactly. If the code and this document ever disagree, the code
> is correct and this document is a bug.

## Overview

`calculate_health_score(user, total_remittance) -> Result<HealthScore, ReportingError>`
returns a score in the range **0..=100**, composed of three independently
computed, weighted components:

| Component   | Weight (max points) | Input it consumes                                                        |
| ----------- | ------------------- | ------------------------------------------------------------------------ |
| Savings     | **0–40**            | Aggregate savings-goal completion: `total_saved / total_target`          |
| Bills       | **0–40**            | Bill-payment compliance: presence of unpaid and of overdue unpaid bills  |
| Insurance   | **0–20**            | Existence of at least one active insurance policy (binary)               |
| **Total**   | **0–100**           | `clamp(savings + bills + insurance, 0, 100)`                             |

The maximum attainable component scores sum to exactly `40 + 40 + 20 = 100`, so
the final clamp to `0..=100` is a defensive guarantee rather than a value that is
normally reached by saturation.

> **Note on the `total_remittance` argument:** it is accepted for API/signature
> stability but is **currently unused** (`_total_remittance` in the source). It
> does not affect the score.

## Components

### Savings score (0–40)

Source: `calculate_savings_score`.

1. Fetch all of the user's goals via the savings_goals contract
   (`get_all_goals`).
2. For each goal, clamp the amounts defensively before summing:
   - `target = clamp(goal.target_amount, 0, i128::MAX / 2)`
   - `saved  = clamp(goal.current_amount, 0, target)`
   - Accumulate `total_target` and `total_saved` using **saturating** addition.
3. **Default case — `total_target == 0`** (the user has no goals, or every goal
   has a zero target): return **20** (a neutral half-of-max default).
4. Otherwise compute completion percentage, clamped to `0..=100`:
   - If `total_saved >= total_target` → `progress = 100`.
   - Else `progress = min((total_saved * 100) / total_target, 100)`, using
     saturating multiply and checked division (division failure → `0`).
5. Convert to points: `score = (progress * 40) / 100`, then `min(score, 40)`.

So at 0% completion the savings score is `0`; at 80% it is `32`; at 100% it is
`40`. With no goals configured it is the default `20`.

### Bills score (0–40)

Source: `calculate_bills_score`.

1. Fetch the user's unpaid bills via the bill_payments contract
   (`get_unpaid_bills(user, 0, 1000)` — up to 1000 unpaid bills are inspected).
2. Decide the score by tier:

| Condition                                  | Score |
| ------------------------------------------ | ----- |
| No unpaid bills at all                     | **40** |
| Has unpaid bills, but **none** are overdue | **35** |
| Has at least one **overdue** unpaid bill   | **20** |

A bill is "overdue" when `bill.due_date < env.ledger().timestamp()`.

This is a **tiered** model (40 / 35 / 20), not a continuous compliance
percentage. A user with no bills receives the perfect-compliance score of 40.

### Insurance score (0–20)

Source: `calculate_insurance_score`.

This is **binary**:

| Condition                         | Score |
| --------------------------------- | ----- |
| At least one active policy exists | **20** |
| No active policies                | **0**  |

It checks only existence (`get_active_policies(user, 0, 1)` — a single-item
page), not coverage amount, premium, or the `coverage_to_premium_ratio` that the
`InsuranceReport` struct exposes elsewhere.

## Clamping

The total is `clamp_score(savings_score + bills_score + insurance_score, 0, 100)`,
where `clamp_score` returns `min` if below `min`, `max` if above `max`, else the
value unchanged. Each component is also independently capped at its own maximum
inside its helper (savings `min(_, 40)`; bills and insurance return fixed
constants), so the components are guaranteed to be in `0..=40`, `0..=40`, and
`0..=20` respectively, and the total in `0..=100`.

## Data availability: Partial / Missing behavior

The `reporting` contract exposes a `DataAvailability` enum (`Complete`,
`Partial`, `Missing`) on several report structs (e.g. `BillComplianceReport`,
`InsuranceReport`) to signal incomplete cross-contract data.

**`HealthScore` does not carry or consult a `DataAvailability` value.**
`calculate_health_score` calls the downstream contracts directly through their
clients and derives each component from the raw results. Consequently:

- **Addresses not configured:** if the reporting contract has no stored
  `ContractAddresses`, `calculate_health_score` returns
  `Err(ReportingError::AddressesNotConfigured)` and produces **no** score. This
  is the only "missing data" outcome the function models explicitly.
- **A downstream contract has no data for the user:** this is treated as a
  legitimate, non-error state and maps to each component's default:
  - no savings goals → savings `20`
  - no unpaid bills → bills `40`
  - no active policies → insurance `0`

  As a result, a brand-new user with no goals, no bills, and no insurance scores
  `20 + 40 + 0 = 60`.
- **A downstream call fails/panics:** the cross-contract call propagates the
  failure and the whole `calculate_health_score` invocation reverts; there is no
  silent `Partial` degradation of the score. If callers need a degradation
  signal, they should read the dedicated report endpoints
  (`get_bill_compliance_report`, `get_insurance_report`, …) which surface
  `DataAvailability`.

## Worked examples

### Typical profile

A user with:

- savings goals at **80%** aggregate completion → `(80 * 40) / 100 = 32`
- unpaid bills present but **none overdue** → `35`
- at least one **active** insurance policy → `20`

**Total:** `32 + 35 + 20 = 87`.

(This is exactly the case asserted in `reporting`'s
`test_calculate_health_score`: `savings_score = 32`, `bills_score = 35`,
`insurance_score = 20`, `score = 87`.)

### Edge profiles

| Profile                                                        | Savings | Bills | Insurance | Total |
| -------------------------------------------------------------- | ------- | ----- | --------- | ----- |
| **Full marks** — 100% savings, no unpaid bills, active policy  | 40      | 40    | 20        | 100   |
| **Brand-new / empty** — no goals, no bills, no policy          | 20      | 40    | 0         | 60    |
| **At-risk** — 0% savings, an overdue bill, no policy           | 0       | 20    | 0         | 20    |
| **All-default** — goals all zero-target, no bills, no policy   | 20      | 40    | 0         | 60    |

The "empty" and "all-default" rows both land at `60` because an absence of
bills counts as perfect compliance (40) and an absence of goals yields the
neutral savings default (20).

## Soroban SDK note for integrators

These contracts target **Soroban SDK `21.7.7`**. `HealthScore` is a
`#[contracttype]` struct; its four fields are `u32`:

```rust
pub struct HealthScore {
    pub score: u32,           // 0..=100
    pub savings_score: u32,   // 0..=40
    pub bills_score: u32,     // 0..=40
    pub insurance_score: u32, // 0..=20
}
```

When decoding cross-contract or off-chain (e.g. via the indexer), expect all
four fields as unsigned 32-bit integers in the ranges above. The component
fields always sum to `score` under normal operation because the per-component
caps already keep the sum at or below `100`.

## Verification

```bash
# Build the docs for the reporting crate and confirm the doc comments compile
cargo doc -p reporting --no-deps

# Re-run the scoring tests that pin the worked example
cargo test -p reporting calculate_health_score
```
