# Insurance Contract

[![Contract](https://img.shields.io/badge/Contract-Insurance-blue)](https://github.com/your-org/remitwise-contracts/tree/main/insurance)

A Soroban smart contract for managing micro-insurance policies with automated premium payments and policy lifecycle management.

## Overview

The Insurance contract enables users to purchase and manage micro-insurance policies, supporting health, emergency, and other coverage types. It handles premium payments, policy activation/deactivation, and provides tools for managing insurance costs within remittance budgets.

## Features

- ✅ **Policy Creation**: Create insurance policies with custom coverage and premiums
- ✅ **Premium Payments**: Track and process monthly premium payments
- ✅ **Policy Management**: Activate, deactivate, and monitor policy status
- ✅ **Cost Tracking**: Calculate total monthly insurance costs
- ✅ **Automated Scheduling**: Track next payment dates

## Data Structures

### InsurancePolicy

```rust
pub struct InsurancePolicy {
    pub id: u32,                    // Unique policy identifier
    pub name: String,               // Policy name/description
    pub coverage_type: String,      // Type of coverage (health, emergency, etc.)
    pub monthly_premium: i128,      // Monthly premium amount
    pub coverage_amount: i128,      // Total coverage amount
    pub active: bool,               // Policy activation status
    pub next_payment_date: u64,     // Next premium due date (Unix timestamp)
}
```

## Functions

### Core Functions

| Function | Description |
|----------|-------------|
| `create_policy` | Create a new insurance policy |
| `pay_premium` | Process monthly premium payment |
| `get_policy` | Retrieve policy details by ID |
| `get_active_policies` | Get all active policies |
| `get_total_monthly_premium` | Calculate total monthly premium costs |
| `deactivate_policy` | Cancel a policy |

### Usage Examples

#### Creating a Health Insurance Policy

```rust
let policy_id = contract.create_policy(
    "Family Health Insurance".to_string(),
    "health".to_string(),
    50,     // monthly premium
    10000   // coverage amount
);
```

#### Monthly Premium Payment

```rust
let success = contract.pay_premium(policy_id);
if success {
    // Premium paid, next payment date updated
}
```

#### Managing Insurance Budget

```rust
let active_policies = contract.get_active_policies();
let total_monthly_cost = contract.get_total_monthly_premium();

// Check if within budget
if total_monthly_cost <= monthly_budget {
    // Proceed with payments
}
```

## Integration Patterns

### With Remittance Split

Integrate insurance payments into automated remittance processing:

```rust
// 1. Calculate split from remittance
let split_amounts = remittance_split.calculate_split(total_remittance);

// 2. Allocate to insurance (split_amounts[3] is insurance portion)
let active_policies = insurance.get_active_policies();

// 3. Pay premiums from allocated amount
for policy in active_policies {
    if split_amounts[3] >= policy.monthly_premium {
        insurance.pay_premium(policy.id);
        split_amounts[3] -= policy.monthly_premium;
    }
}
```

### Policy Lifecycle Management

```rust
// Create multiple policies
let policies = vec![
    ("Health Insurance", "health", 50, 10000),
    ("Emergency Fund", "emergency", 25, 5000),
    ("Life Insurance", "life", 30, 15000),
];

let mut policy_ids = vec![];
for (name, coverage_type, premium, coverage) in policies {
    let id = contract.create_policy(
        name.to_string(),
        coverage_type.to_string(),
        premium,
        coverage
    );
    policy_ids.push(id);
}

// Monitor and maintain policies
for &id in &policy_ids {
    if let Some(policy) = contract.get_policy(id) {
        // Check if payment is due
        if current_time >= policy.next_payment_date {
            // Handle overdue payment
        }
    }
}
```

## Testing

Run the contract tests:

```bash
cd insurance
cargo test
```

Run with verbose output:

```bash
cargo test -- --nocapture
```

## Deployment

Build for deployment:

```bash
cargo build --release --target wasm32-unknown-unknown
```

The compiled WASM file will be available at:
`target/wasm32-unknown-unknown/release/insurance.wasm`

## API Reference

For complete API documentation, see [Insurance API](../../docs/api/insurance.md).

## Error Handling

The contract uses boolean returns and Option types for error handling:

- `create_policy`: Always succeeds (returns new policy ID)
- `pay_premium`: Returns `false` if policy not found or inactive
- `get_policy`: Returns `None` if policy doesn't exist
- `get_active_policies`: Always succeeds (may return empty vector)
- `get_total_monthly_premium`: Always succeeds (may return 0)
- `deactivate_policy`: Returns `false` if policy not found

## Security Considerations

- Policy IDs are auto-incremented to prevent conflicts
- Premium amounts are validated as positive values
- Coverage amounts are validated as positive values
- Only active policies can receive premium payments
- Next payment dates are automatically calculated

## Future Enhancements

- [ ] Policy renewal automation
- [ ] Claims processing integration
- [ ] Premium payment history
- [ ] Policy comparison tools
- [ ] Multi-coverage policies
- [ ] Integration with insurance providers