# Remittance Split Contract

A Soroban smart contract for configuring and calculating remittance fund allocations across spending, savings, bills, and insurance categories.

## Overview

The Remittance Split contract manages percentage-based allocations for incoming remittances, automatically distributing funds according to user-defined ratios for different financial categories.

## Features

- Configure allocation percentages (spending, savings, bills, insurance)
- Calculate split amounts from total remittance
- Update split configurations
- Access control for configuration management
- Event emission for audit trails
- Backward compatibility with vector-based storage

## Quickstart

This section provides a minimal example of how to interact with the Remittance Split contract.

**Gotchas:**
- The configured percentages MUST sum up exactly to 100.
- `initialize_split` must be called with a valid `nonce` for replay protection.
- To execute actual underlying asset transfers, use `distribute_usdc` rather than just calculating numbers.

### Write Example: Initializing the Split
*Note: This is pseudo-code demonstrating the Soroban Rust SDK CLI or client approach.*
```rust

let success = client.initialize_split(
    &owner_address,
    &0,  
    &50, 
    &30, 
    &15, 
    &5   
);

```

### Read Example: Fetching the Configuration
```rust

let config = client.get_config();

```

## API Reference

### Data Structures

#### SplitConfig

```rust
pub struct SplitConfig {
    pub owner: Address,
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
    pub initialized: bool,
}
```

### Functions

#### `initialize_split(env, owner, spending_percent, savings_percent, bills_percent, insurance_percent) -> bool`

Initializes a remittance split configuration.

**Parameters:**

- `owner`: Address of the split owner (must authorize)
- `spending_percent`: Percentage for spending (0-100)
- `savings_percent`: Percentage for savings (0-100)
- `bills_percent`: Percentage for bills (0-100)
- `insurance_percent`: Percentage for insurance (0-100)

**Returns:** True on success

**Panics:** If percentages don't sum to 100 or already initialized

#### `update_split(env, caller, spending_percent, savings_percent, bills_percent, insurance_percent) -> bool`

Updates an existing split configuration.

**Parameters:**

- `caller`: Address of the caller (must be owner)
- `spending_percent`: New spending percentage
- `savings_percent`: New savings percentage
- `bills_percent`: New bills percentage
- `insurance_percent`: New insurance percentage

**Returns:** True on success

**Panics:** If caller not owner, percentages invalid, or not initialized

#### `get_split(env) -> Vec<u32>`

Gets the current split percentages.

**Returns:** Vector [spending, savings, bills, insurance] percentages

#### `get_config(env) -> Option<SplitConfig>`

Gets the full split configuration.

**Returns:** SplitConfig struct or None if not initialized

#### `calculate_split(env, total_amount) -> Vec<i128>`

Calculates split amounts from a total remittance amount.

**Parameters:**

- `total_amount`: Total amount to split (must be positive)

**Returns:** Vector [spending, savings, bills, insurance] amounts

**Panics:** If total_amount not positive

## Usage Examples

### Initializing Split Configuration

```rust
// Initialize with 50% spending, 30% savings, 15% bills, 5% insurance
let success = remittance_split::initialize_split(
    env,
    user_address,
    50, // spending
    30, // savings
    15, // bills
    5,  // insurance
);
```

### Calculating Split Amounts

```rust
// Calculate allocation for 1000 XLM remittance
let amounts = remittance_split::calculate_split(env, 1000_0000000);

// amounts = [500_0000000, 300_0000000, 150_0000000, 50_0000000]
let spending_amount = amounts.get(0).unwrap();
let savings_amount = amounts.get(1).unwrap();
let bills_amount = amounts.get(2).unwrap();
let insurance_amount = amounts.get(3).unwrap();
```

### Updating Configuration

```rust
// Update to 40% spending, 40% savings, 10% bills, 10% insurance
let success = remittance_split::update_split(
    env,
    user_address,
    40, 40, 10, 10
);
```

## Events

- `SplitEvent::Initialized`: When split is initialized
- `SplitEvent::Updated`: When split is updated
- `SplitEvent::Calculated`: When split calculation is performed

## Integration Patterns

### With Other Contracts

The split contract serves as a central allocation engine:

```rust
// Get split amounts
let split = remittance_split::calculate_split(env, remittance_amount);

// Allocate to savings goals
savings_goals::add_to_goal(env, user, goal_id, split.get(1).unwrap())?;

// Create bill payments
bill_payments::create_bill(env, user, "Monthly Bills".into(), split.get(2).unwrap(), due_date, false, 0)?;

// Pay insurance premiums
insurance::pay_premium(env, user, policy_id);
```

### Automated Remittance Processing

```rust
// Process incoming remittance
fn process_remittance(env: Env, user: Address, amount: i128) {
    let split = remittance_split::calculate_split(env, amount);

    // Auto-allocate funds
    allocate_to_savings(env, user, split.get(1).unwrap());
    allocate_to_bills(env, user, split.get(2).unwrap());
    allocate_to_insurance(env, user, split.get(3).unwrap());
}
```

## Security Considerations

- Owner authorization required for configuration changes
- Percentage validation ensures allocations sum to 100%
- Initialization check prevents duplicate setup
- Access control prevents unauthorized modifications
