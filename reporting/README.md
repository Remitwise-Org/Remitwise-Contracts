# Reporting Contract

Financial reporting and insights contract for the RemitWise platform.

## Overview

Generates on-chain financial health reports by aggregating data from the
remittance\_split, savings\_goals, bill\_payments, and insurance contracts.


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

