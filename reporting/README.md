# Reporting Contract

Financial reporting and insights contract for the RemitWise platform.

## Overview

Generates on-chain financial health reports by aggregating data from the
remittance\_split, savings\_goals, bill\_payments, and insurance contracts.

## Financial Health Score

The contract calculates a comprehensive financial health score (0-100) based on three components:

### Score Components

- **Savings Score (0-40 points)**: Based on savings goal completion percentage
- **Bills Score (0-40 points)**: Based on bill payment compliance
- **Insurance Score (0-20 points)**: Based on active insurance coverage

### Arithmetic Safety & Normalization

The health score calculation implements hardened arithmetic to ensure security and predictability:

#### Overflow Protection
- Uses saturating arithmetic for amount summations
- Safe division prevents intermediate overflow in percentage calculations
- Individual amounts are clamped to reasonable bounds

#### Bounds Guarantees
- Overall score is always bounded [0, 100]
- Component scores never exceed their maximum values
- Progress percentages are clamped [0, 100]

#### Edge Case Handling
- Zero savings targets result in default score (20 points)
- Negative amounts are clamped to zero
- Extreme input values don't cause calculation failures

#### Security Properties
- Deterministic output for identical inputs
- No external dependencies on ledger state
- Cross-contract calls use configured addresses only

### Example Calculation

For a user with:
- 80% savings goal completion → 32 savings points
- Unpaid bills (none overdue) → 35 bills points  
- Active insurance policy → 20 insurance points

**Total Score**: 32 + 35 + 20 = 87

## Trend Analysis

### `get_trend_analysis`

Compares two scalar amounts and returns a `TrendData` struct:

```
TrendData {
    current_amount:    i128,
    previous_amount:   i128,
    change_amount:     i128,   // current - previous
    change_percentage: i32,    // signed %; 100 when previous == 0 and current > 0
}
```

**Determinism guarantee**: output depends only on `current_amount` and
`previous_amount`; ledger timestamp, user address, and call order have no
effect.

### `get_trend_analysis_multi`

Accepts a `Vec<(u64, i128)>` of `(period_key, amount)` pairs and returns
`Vec<TrendData>` with one entry per adjacent pair (`len - 1` entries).
Returns an empty Vec when fewer than two points are supplied.

**Determinism guarantee**: identical `history` input always produces identical
output regardless of call order, ledger state, or caller identity.

## Running Tests
