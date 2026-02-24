---
name: "[SECURITY-002] Implement Reentrancy Protection in Orchestrator"
about: Add reentrancy guard to prevent state corruption during cross-contract calls
title: "[SECURITY-002] Implement Reentrancy Protection in Orchestrator"
labels: security, high-priority, orchestrator
assignees: ''
---

## Security Issue

**Severity:** HIGH  
**Component:** orchestrator contract  
**Threat ID:** T-RE-01

## Description

The orchestrator makes multiple cross-contract calls without reentrancy protection. If a downstream contract calls back to the orchestrator during execution, state could be corrupted leading to duplicate operations or financial loss.

### Affected Functions
- `execute_remittance_flow()`
- `deposit_to_savings()`
- `execute_bill_payment_internal()`
- `pay_insurance_premium()`

## Attack Scenario

1. Attacker deploys malicious contract implementing savings/bills/insurance interface
2. Attacker configures orchestrator to use malicious contract
3. Attacker calls `execute_remittance_flow()`
4. Malicious contract receives call, calls back to orchestrator mid-execution
5. Orchestrator state is modified during execution
6. Duplicate allocations or state corruption occurs

## Proposed Solution

Implement reentrancy guard using storage flag:

```rust
const REENTRANCY_GUARD: Symbol = symbol_short!("RE_GUARD");

fn check_reentrancy(env: &Env) {
    let guard: bool = env
        .storage()
        .instance()
        .get(&REENTRANCY_GUARD)
        .unwrap_or(false);
    
    if guard {
        panic!("Reentrancy detected");
    }
    
    env.storage().instance().set(&REENTRANCY_GUARD, &true);
}

fn clear_reentrancy(env: &Env) {
    env.storage().instance().set(&REENTRANCY_GUARD, &false);
}

pub fn execute_remittance_flow(
    env: Env,
    caller: Address,
    // ... params
) -> Result<(), OrchestratorError> {
    check_reentrancy(&env);
    
    // ... existing logic
    
    clear_reentrancy(&env);
    Ok(())
}
```

## Acceptance Criteria

- [ ] Reentrancy guard implemented using storage flag
- [ ] All cross-contract call functions protected
- [ ] Guard is set before external calls
- [ ] Guard is cleared after function completes
- [ ] Guard is cleared even if function panics (use defer pattern)
- [ ] Tests verify reentrancy attempts are blocked
- [ ] Gas cost impact measured and documented
- [ ] No performance degradation for normal operations

## Implementation Tasks

- [ ] Add REENTRANCY_GUARD constant
- [ ] Implement `check_reentrancy()` helper
- [ ] Implement `clear_reentrancy()` helper
- [ ] Add guard to `execute_remittance_flow()`
- [ ] Add guard to all cross-contract call functions
- [ ] Ensure guard is cleared on panic (consider defer pattern)
- [ ] Write unit tests for reentrancy protection
- [ ] Write integration tests with malicious contracts
- [ ] Measure gas cost impact
- [ ] Update documentation

## Testing Requirements

- Test normal execution (no reentrancy)
- Test reentrancy attempt is blocked
- Test guard is cleared after successful execution
- Test guard is cleared after panic
- Test multiple concurrent calls (if applicable)
- Measure gas cost overhead

## Alternative Approaches

1. **Checks-Effects-Interactions Pattern:** Refactor to complete all state changes before external calls
2. **Mutex Lock:** Use more sophisticated locking mechanism
3. **Call Depth Tracking:** Track call depth instead of binary flag

## Estimated Effort

3-5 days

## Related Issues

- Relates to THREAT_MODEL.md Section 3.6
- Blocks mainnet deployment
