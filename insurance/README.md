# Insurance Contract

A Soroban smart contract for managing insurance policies with premium tracking, payment management, and access control.

## Overview

The Insurance contract enables users to create and manage insurance policies, track premium payments, and maintain policy status. It supports monthly premium payments and policy deactivation.

## Features

- Create insurance policies with monthly premiums
- Track premium payment schedules
- Automatic next payment date calculation
- Policy activation/deactivation
- Access control for policy owners
- Event emission for audit trails
- Storage TTL management

## Quickstart

This section provides a minimal example of how to interact with the Insurance contract.

**Gotchas:**
- Amounts are specified in the lowest denomination (e.g., stroops for XLM).
- The `pay_premium` function assumes authorization succeeds and sets `next_payment_date`. Ensure you handle asset transfers depending on your implementation.
- `deactivate_policy` stops future premium calculations but cannot be reversed in the current implementation.

### Write Example: Creating a Policy
*Note: This is pseudo-code demonstrating the Soroban Rust SDK CLI or client approach.*
```rust

let policy_id = client.create_policy(
    &owner_address,
    &String::from_str(&env, "Life Insurance"),
    &String::from_str(&env, "Life"),
    &100_0000000,                            
    &10000_0000000,                         
    &String::from_str(&env, "XLM")          
);

```

### Read Example: Fetching Active Policies
```rust

let limit = 10;
let cursor = 0;
let page = client.get_active_policies(&owner_address, &cursor, &limit);

```

## API Reference

### Data Structures

#### InsurancePolicy

```rust
pub struct InsurancePolicy {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub coverage_type: String,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub active: bool,
    pub next_payment_date: u64,
}
```

### Functions

#### `create_policy(env, owner, name, coverage_type, monthly_premium, coverage_amount) -> u32`

Creates a new insurance policy.

**Parameters:**

- `owner`: Address of the policy owner (must authorize)
- `name`: Policy name
- `coverage_type`: Type of coverage (e.g., "health", "emergency")
- `monthly_premium`: Monthly premium amount (must be positive)
- `coverage_amount`: Total coverage amount (must be positive)

**Returns:** Policy ID

**Panics:** If inputs are invalid or owner doesn't authorize

#### `pay_premium(env, caller, policy_id) -> bool`

Pays monthly premium for a policy.

**Parameters:**

- `caller`: Address of the caller (must be policy owner)
- `policy_id`: ID of the policy

**Returns:** True on success

**Panics:** If caller is not owner, policy not found, or policy inactive

#### `get_policy(env, policy_id) -> Option<InsurancePolicy>`

Retrieves a policy by ID.

**Parameters:**

- `policy_id`: ID of the policy

**Returns:** InsurancePolicy struct or None

#### `get_active_policies(env, owner) -> Vec<InsurancePolicy>`

Gets all active policies for an owner.

**Parameters:**

- `owner`: Address of the policy owner

**Returns:** Vector of active InsurancePolicy structs

#### `get_total_monthly_premium(env, owner) -> i128`

Calculates total monthly premium for all active policies of an owner.

**Parameters:**

- `owner`: Address of the policy owner

**Returns:** Total monthly premium amount

#### `deactivate_policy(env, caller, policy_id) -> bool`

Deactivates a policy.

**Parameters:**

- `caller`: Address of the caller (must be policy owner)
- `policy_id`: ID of the policy

**Returns:** True on success

**Panics:** If caller is not owner or policy not found

## Usage Examples

### Creating a Policy

```rust
// Create a health insurance policy
let policy_id = insurance::create_policy(
    env,
    user_address,
    "Health Insurance".into(),
    "health".into(),
    100_0000000, // 100 XLM monthly
    10000_0000000, // 10,000 XLM coverage
);
```

### Paying Premium

```rust
// Pay monthly premium
let success = insurance::pay_premium(env, user_address, policy_id);
```

### Querying Policies

```rust
// Get all active policies
let active_policies = insurance::get_active_policies(env, user_address);

// Get total monthly premium
let total_premium = insurance::get_total_monthly_premium(env, user_address);
```

## Events

- `InsuranceEvent::PolicyCreated`: When a policy is created
- `InsuranceEvent::PremiumPaid`: When a premium is paid
- `InsuranceEvent::PolicyDeactivated`: When a policy is deactivated

## Integration Patterns

### With Remittance Split

Insurance premiums can be automatically allocated from remittance splits:

```rust
let split_amounts = remittance_split::calculate_split(env, total_remittance);
let insurance_allocation = split_amounts.get(3).unwrap(); // insurance percentage

// Use allocation for premium payments
```

### With Bill Payments

Insurance policies can generate corresponding bill entries for premium tracking.

## Security Considerations

- Owner authorization required for all operations
- Input validation for positive amounts
- Policy state validation before operations
- Access control prevents unauthorized modifications
