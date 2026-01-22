# Remittance Split Contract API Reference

## Overview

The Remittance Split contract handles automatic allocation of remittance funds into different categories: spending, savings, bills, and insurance.

## Functions

### initialize_split

Initializes a remittance split configuration.

**Signature:**

```rust
pub fn initialize_split(
    env: Env,
    spending_percent: u32,
    savings_percent: u32,
    bills_percent: u32,
    insurance_percent: u32,
) -> bool
```

**Parameters:**

- `spending_percent`: Percentage allocated to spending (0-100)
- `savings_percent`: Percentage allocated to savings (0-100)
- `bills_percent`: Percentage allocated to bills (0-100)
- `insurance_percent`: Percentage allocated to insurance (0-100)

**Returns:**

- `true`: Configuration was successfully set (percentages must sum to 100)
- `false`: Percentages do not sum to 100

**Errors:** No explicit errors; returns false if percentages don't sum to 100.

### get_split

Gets the current split configuration.

**Signature:**

```rust
pub fn get_split(env: &Env) -> Vec<u32>
```

**Returns:** A vector of four u32 values representing percentages: [spending, savings, bills, insurance]. Returns default [50, 30, 15, 5] if not initialized.

**Errors:** No errors; always returns a vector.

### calculate_split

Calculates split amounts from a total remittance amount.

**Signature:**

```rust
pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128>
```

**Parameters:**

- `total_amount`: The total remittance amount to split

**Returns:** A vector of four i128 values representing split amounts: [spending, savings, bills, insurance]. The last amount (insurance) is calculated as remainder to ensure total sums correctly.

**Errors:** No errors; always returns a vector.
