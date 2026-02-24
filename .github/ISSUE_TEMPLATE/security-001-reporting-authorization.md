---
name: "[SECURITY-001] Add Authorization to Reporting Contract Queries"
about: Implement caller verification in reporting contract to prevent unauthorized data access
title: "[SECURITY-001] Add Authorization to Reporting Contract Queries"
labels: security, high-priority, reporting
assignees: ''
---

## Security Issue

**Severity:** HIGH  
**Component:** reporting contract  
**Threat ID:** T-UA-01

## Description

The reporting contract currently allows any caller to query sensitive financial data for any user without authorization checks. This creates a critical privacy vulnerability where attackers can build complete financial profiles of users.

### Affected Functions
- `get_remittance_summary()`
- `get_savings_report()`
- `get_bill_compliance_report()`
- `get_insurance_coverage_report()`
- `generate_financial_health_report()`

## Attack Scenario

1. Attacker identifies target user address from public transactions
2. Calls reporting functions with target address
3. Retrieves complete financial profile including balances, goals, bills, policies
4. Uses information for social engineering or targeted attacks

## Proposed Solution

Add caller verification to all reporting query functions:

```rust
pub fn get_remittance_summary(
    env: Env,
    caller: Address,
    user: Address,
    total_amount: i128,
    period_start: u64,
    period_end: u64,
) -> RemittanceSummary {
    caller.require_auth();
    
    // Verify caller is authorized to view user data
    if caller != user {
        // Check ACL or admin status
        let is_authorized = Self::check_data_access_permission(&env, &caller, &user);
        if !is_authorized {
            panic!("Unauthorized access to user financial data");
        }
    }
    
    // ... existing logic
}
```

## Acceptance Criteria

- [ ] All reporting query functions require caller authentication
- [ ] Caller must be the user or have explicit permission
- [ ] Add access control list (ACL) support for shared access (optional)
- [ ] Add admin override capability for support purposes
- [ ] Update all tests to verify authorization checks
- [ ] Add negative tests for unauthorized access attempts
- [ ] Update documentation with authorization requirements
- [ ] Add events for unauthorized access attempts

## Implementation Tasks

- [ ] Add `caller: Address` parameter to all query functions
- [ ] Implement `check_data_access_permission()` helper function
- [ ] Add ACL storage structure (optional)
- [ ] Update function signatures in client code
- [ ] Write unit tests for authorization
- [ ] Write integration tests
- [ ] Update API documentation
- [ ] Add migration guide for existing integrations

## Testing Requirements

- Test authorized access (caller == user)
- Test unauthorized access (caller != user, no permission)
- Test admin access override
- Test ACL-based access (if implemented)
- Test performance impact of authorization checks

## Estimated Effort

2-3 days

## Related Issues

- Relates to THREAT_MODEL.md Section 3.1
- Blocks mainnet deployment
