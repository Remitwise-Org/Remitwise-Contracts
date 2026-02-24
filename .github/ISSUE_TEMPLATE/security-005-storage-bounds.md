---
name: "[SECURITY-005] Implement Storage Bounds and Entity Limits"
about: Add per-user limits on entity creation to prevent storage bloat DoS
title: "[SECURITY-005] Implement Storage Bounds and Entity Limits"
labels: security, medium-priority, all-contracts
assignees: ''
---

## Security Issue

**Severity:** MEDIUM  
**Component:** All contracts (savings_goals, bill_payments, insurance)  
**Threat ID:** T-DOS-01

## Description

Maps can grow unbounded with no limits on entity creation. Attackers can create excessive entities to exhaust storage, increase costs, and potentially cause denial of service for legitimate users.

### Affected Contracts
- `savings_goals` - GOALS map
- `bill_payments` - BILLS map
- `insurance` - POLICIES map

## Attack Scenario

1. Attacker creates maximum number of goals/bills/policies
2. Performs operations to generate audit log entries
3. Storage costs increase dramatically
4. Legitimate users unable to create new entities
5. Contract becomes expensive to maintain

## Proposed Solution

Add per-user entity limits:

```rust
const MAX_GOALS_PER_USER: u32 = 100;
const MAX_BILLS_PER_USER: u32 = 200;
const MAX_POLICIES_PER_USER: u32 = 50;

pub fn create_goal(
    env: Env,
    owner: Address,
    name: String,
    target_amount: i128,
    target_date: u64,
) -> u32 {
    owner.require_auth();
    Self::require_not_paused(&env, pause_functions::CREATE_GOAL);
    
    // ... existing validation
    
    // Count existing goals for owner
    let goals: Map<u32, SavingsGoal> = env
        .storage()
        .instance()
        .get(&symbol_short!("GOALS"))
        .unwrap_or_else(|| Map::new(&env));
    
    let mut count = 0u32;
    for (_, goal) in goals.iter() {
        if goal.owner == owner {
            count += 1;
        }
    }
    
    if count >= MAX_GOALS_PER_USER {
        panic!("Maximum goals per user exceeded");
    }
    
    // ... existing logic
}
```

## Acceptance Criteria

- [ ] Maximum entities per user defined for each contract
- [ ] Creation functions enforce limits
- [ ] Error messages indicate limit reached
- [ ] Tests verify limits are enforced
- [ ] Admin function to adjust limits (optional)
- [ ] Limits documented in README
- [ ] Performance impact measured

## Implementation Tasks

### Savings Goals Contract
- [ ] Add MAX_GOALS_PER_USER constant (100)
- [ ] Add count check to `create_goal()`
- [ ] Write tests for limit enforcement

### Bill Payments Contract
- [ ] Add MAX_BILLS_PER_USER constant (200)
- [ ] Add count check to `create_bill()`
- [ ] Write tests for limit enforcement

### Insurance Contract
- [ ] Add MAX_POLICIES_PER_USER constant (50)
- [ ] Add count check to `create_policy()`
- [ ] Write tests for limit enforcement

### Optional Enhancements
- [ ] Add admin function to adjust limits
- [ ] Store limits in contract storage (configurable)
- [ ] Add per-user limit overrides for premium users
- [ ] Implement storage quota system

## Testing Requirements

- Test entity creation up to limit
- Test entity creation at limit (should fail)
- Test entity creation after deletion (should succeed)
- Test limit enforcement for multiple users
- Test performance impact of counting
- Test admin limit adjustment (if implemented)

## Recommended Limits

| Entity Type | Limit | Rationale |
|-------------|-------|-----------|
| Savings Goals | 100 | Most users have <10 goals |
| Bills | 200 | Covers monthly + recurring bills |
| Insurance Policies | 50 | Typical user has <5 policies |

## Performance Considerations

Counting entities requires iterating the map. Consider:
- Caching count in storage per user
- Using separate counter map
- Implementing pagination for large maps

## Estimated Effort

3-4 days

## Related Issues

- Relates to THREAT_MODEL.md Section 3.3
- Should be completed before mainnet deployment
