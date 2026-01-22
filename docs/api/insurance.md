# Insurance Contract API Reference

## Overview

The Insurance contract manages micro-insurance policies, premium payments, and policy lifecycle for the RemitWise platform.

## Data Structures

### InsurancePolicy

```rust
pub struct InsurancePolicy {
    pub id: u32,
    pub name: String,
    pub coverage_type: String, // "health", "emergency", etc.
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub active: bool,
    pub next_payment_date: u64, // Unix timestamp
}
```

## Functions

### create_policy

Creates a new insurance policy.

**Signature:**
```rust
pub fn create_policy(
    env: Env,
    name: String,
    coverage_type: String,
    monthly_premium: i128,
    coverage_amount: i128,
) -> u32
```

**Parameters:**
- `name`: Name of the policy (e.g., "Health Insurance")
- `coverage_type`: Type of coverage (e.g., "health", "emergency")
- `monthly_premium`: Monthly premium amount (must be positive)
- `coverage_amount`: Total coverage amount (must be positive)

**Returns:** The ID of the created policy. The policy is set as active with next payment date 30 days from now.

**Errors:** This function does not return errors; it always succeeds by creating a new policy.

### pay_premium

Pays monthly premium for a policy.

**Signature:**
```rust
pub fn pay_premium(env: Env, policy_id: u32) -> bool
```

**Parameters:**
- `policy_id`: ID of the policy to pay premium for

**Returns:**
- `true`: Premium was successfully paid, next payment date updated to 30 days from now
- `false`: Policy not found or not active

**Errors:** No explicit errors; returns false for invalid operations.

### get_policy

Retrieves a policy by ID.

**Signature:**
```rust
pub fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy>
```

**Parameters:**
- `policy_id`: ID of the policy to retrieve

**Returns:**
- `Some(InsurancePolicy)`: The policy struct if found
- `None`: If the policy does not exist

**Errors:** No errors; returns None for non-existent policies.

### get_active_policies

Gets all active policies.

**Signature:**
```rust
pub fn get_active_policies(env: Env) -> Vec<InsurancePolicy>
```

**Returns:** A vector of all active `InsurancePolicy` structs. Returns an empty vector if no active policies exist.

**Errors:** No errors; always returns a vector.

### get_total_monthly_premium

Gets the total monthly premium for all active policies.

**Signature:**
```rust
pub fn get_total_monthly_premium(env: Env) -> i128
```

**Returns:** The total monthly premium amount (i128) for all active policies. Returns 0 if no active policies exist.

**Errors:** No errors; always returns a valid amount.

### deactivate_policy

Deactivates a policy.

**Signature:**
```rust
pub fn deactivate_policy(env: Env, policy_id: u32) -> bool
```

**Parameters:**
- `policy_id`: ID of the policy to deactivate

**Returns:**
- `true`: Policy was successfully deactivated
- `false`: Policy not found

**Errors:** No explicit errors; returns false for non-existent policies.