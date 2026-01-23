# Security Documentation

This document outlines the security patterns and best practices implemented across all Remitwise smart contracts.

## Overview

All contracts in this repository follow Soroban security guidelines and implement comprehensive security measures including:

- **Access Control**: Owner-based authorization using `require_auth()`
- **Input Validation**: Parameter validation with explicit bounds checking
- **Storage Security**: TTL management to prevent data expiration
- **Audit Trail**: Event emissions for all state-changing operations

## Security Patterns

### 1. Access Control

All state-changing functions require authorization from the appropriate party:

```rust
// Owner must authorize the transaction
owner.require_auth();

// Verify caller is the owner of the resource
if resource.owner != caller {
    panic!("Only the owner can perform this action");
}
```

**Implemented in:**
- `bill_payments`: Bill owner authorization for create/pay operations
- `insurance`: Policy owner authorization for create/pay/deactivate operations
- `remittance_split`: Split owner authorization for initialize/update operations
- `savings_goals`: Goal owner authorization for create/add/withdraw/lock operations

### 2. Input Validation

All user inputs are validated before processing:

| Contract | Validations |
|----------|-------------|
| `bill_payments` | `amount > 0`, `frequency_days > 0` for recurring |
| `insurance` | `monthly_premium > 0`, `coverage_amount > 0` |
| `remittance_split` | `percentages sum to 100`, `total_amount > 0` |
| `savings_goals` | `target_amount > 0`, `amount > 0`, `amount <= balance` |

### 3. Arithmetic Safety

- **Overflow Protection**: Enabled via `overflow-checks = true` in `Cargo.toml`
- **Division Safety**: Integer division with remainder handling (e.g., `remittance_split`)
- **Precision Handling**: Remainder allocated to final category to prevent rounding loss

### 4. Storage Security

All contracts implement storage TTL management:

```rust
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280;  // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400;        // ~30 days

fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}
```

### 5. Event Emissions

All state-changing operations emit events for auditability:

| Contract | Events |
|----------|--------|
| `bill_payments` | `BillCreated`, `BillPaid` |
| `insurance` | `PolicyCreated`, `PremiumPaid`, `PolicyDeactivated` |
| `remittance_split` | `Initialized`, `Updated`, `Calculated` |
| `savings_goals` | `GoalCreated`, `FundsAdded`, `FundsWithdrawn`, `GoalCompleted`, `GoalLocked`, `GoalUnlocked` |

## Data Ownership Model

Each contract implements per-item ownership:

```rust
pub struct Bill {
    pub id: u32,
    pub owner: Address,  // Owner who can manage this bill
    // ... other fields
}
```

This ensures:
- Users can only access their own data
- No cross-user data leakage
- Clear accountability for all operations

## Error Handling

Contracts use explicit `panic!` with descriptive messages for error conditions:

```rust
if amount <= 0 {
    panic!("Amount must be positive");
}
```

This provides:
- Clear error messages for debugging
- Transaction reversion on invalid operations
- No silent failures

## Best Practices Followed

1. **Principle of Least Privilege**: Users can only access their own resources
2. **Fail-Safe Defaults**: Operations fail if authorization is missing
3. **Defense in Depth**: Multiple validation layers (auth + input validation)
4. **Explicit Over Implicit**: All validations and checks are explicit
5. **Audit Trail**: All operations are logged via events

## Reporting Security Issues

If you discover a security vulnerability, please report it responsibly by:

1. **Do not** create a public GitHub issue
2. Email the security team with details
3. Allow time for the issue to be addressed before disclosure

## Security Checklist

- [x] Access control implemented on all state-changing functions
- [x] Input validation for all user-provided parameters
- [x] Overflow protection enabled
- [x] Storage TTL management implemented
- [x] Events emitted for audit trail
- [x] Owner-based data isolation
- [x] Explicit error handling with descriptive messages
