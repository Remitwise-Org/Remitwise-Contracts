# Model Overview

This document provides a concise, concrete description of the **Financial Health Score Model** used by the `reporting` contract. It is intended for **operators** and **downstream integrators** who need to understand how the score is calculated, its inputs, weighting, and edge‑case behavior.

## What the Model Computes

- A single integer `score` in the inclusive range **0‑100**.
- The score is a weighted sum of three components:
  - **Savings** – up to 40 points.
  - **Bills** – up to 40 points.
  - **Insurance** – up to 20 points.

The components are calculated independently and then clamped to their maximum points before adding. The final sum is clamped to `0..=100` as a defensive guarantee.

## Component Definitions

| Component | Weight (max) | Input Source | Calculation Details |
|---|---|---|---|
| **Savings** | **0‑40** | `savings_goals` contract via `get_all_goals` | 1. Sum `target_amount` and `current_amount` of all goals (saturating arithmetic). 2. If `total_target == 0` → default **20** points (neutral). 3. Otherwise `progress = min((saved * 100) / target, 100)`. 4. `score = min((progress * 40) / 100, 40)` |
| **Bills** | **0‑40** | `bill_payments` contract via `get_unpaid_bills` (up to 1000 bills) | Tiered scoring: no unpaid → **40**; unpaid but none overdue → **35**; at least one overdue → **20** |
| **Insurance** | **0‑20** | `insurance` contract via `get_active_policies` (single‑page fetch) | Binary: at least one active policy → **20**; otherwise **0** |

## Edge Cases & Defaults

- **No Savings Goals** – returns the neutral default of **20** points (half of the maximum). This prevents a new user from being penalised.
- **No Bills** – treated as perfect compliance → **40** points.
- **No Insurance** – yields **0** points.
- The `total_remittance` argument is currently unused (kept for API stability).

## Worked Example

A typical user profile:
- Savings completion: **80 %** → `40 * 0.8 = 32` points.
- Unpaid bills present, none overdue → **35** points.
- At least one active insurance policy → **20** points.

**Total Score:** `32 + 35 + 20 = 87`.

The same example is covered by the unit test `test_calculate_health_score` in the `reporting` crate.

## Verification

```bash
# Build the contract for WASM (no std, no panic)
cargo build --release --target wasm32-unknown-unknown -p reporting

# Run the health‑score unit test
cargo test -p reporting calculate_health_score
```

Both commands should succeed without warnings (`cargo clippy -- -D warnings`).
