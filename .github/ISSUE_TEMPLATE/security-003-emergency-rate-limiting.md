---
name: "[SECURITY-003] Add Rate Limiting to Emergency Transfers"
about: Enforce cooldown and transfer limits in emergency mode
title: "[SECURITY-003] Add Rate Limiting to Emergency Transfers"
labels: security, high-priority, family-wallet
assignees: ''
---

## Security Issue

**Severity:** HIGH
**Component:** family_wallet contract
**Threat ID:** T-EC-02

## Description

Emergency mode allows unlimited transfers without multi-sig protection and no cooldown enforcement. If an Owner/Admin account is compromised, attackers can rapidly drain all family wallet funds before detection.

### Affected Functions
- `execute_emergency_transfer_now()`
- `set_emergency_mode()`
- `configure_emergency()`

## Attack Scenario

1. Attacker compromises Owner/Admin private key
2. Activates emergency mode via `set_emergency_mode(true)`
3. Rapidly calls `execute_emergency_transfer_now()` multiple times
4. Transfers all funds to attacker-controlled addresses
5. Deactivates emergency mode to cover tracks

## Proposed Solution

Enforce cooldown between emergency transfers:

```rust
pub fn execute_emergency_transfer_now(
    env: Env,
    proposer: Address,
    token: Address,
    recipient: Address,
    amount: i128,
) -> u64 {
    proposer.require_auth();
    Self::require_not_paused(&env);

    // Check emergency mode is active
    let em_mode: bool = env
        .storage()
        .instance()
        .get(&symbol_short!("EM_MODE"))
        .unwrap_or(false);

    if !em_mode {
        panic!("Emergency mode not active");
    }

    // Get emergency config
    let em_config: EmergencyConfig = env
        .storage()
        .instance()
        .get(&symbol_short!("EM_CONF"))
        .expect("Emergency config not set");

    // Check cooldown
    let last_transfer: u64 = env
        .storage()
        .instance()
        .get(&symbol_short!("EM_LAST"))
        .unwrap_or(0);

    let current_time = env.ledger().timestamp();
    if current_time < last_transfer + em_config.cooldown {
        panic!("Emergency transfer cooldown not elapsed");
    }

    // Check amount limit
    if amount > em_config.max_amount {
        panic!("Emergency transfer exceeds maximum amount");
    }

    // Update last transfer time
    env.storage()
        .instance()
        .set(&symbol_short!("EM_LAST"), &current_time);

    // ... existing transfer logic
}
```

## Acceptance Criteria

- [ ] Cooldown enforced between emergency transfers
- [ ] Maximum transfer amount enforced
- [ ] Emergency config includes cooldown parameter
- [ ] Emergency config includes max_amount parameter
- [ ] Last transfer timestamp tracked in storage
- [ ] Tests verify cooldown enforcement
- [ ] Tests verify amount limit enforcement
- [ ] Events emitted for emergency transfers
- [ ] Documentation updated with rate limiting details

## Implementation Tasks

- [ ] Add cooldown check to `execute_emergency_transfer_now()`
- [ ] Add amount limit check
- [ ] Store last transfer timestamp
- [ ] Update `EmergencyConfig` struct if needed
- [ ] Add cooldown parameter to `configure_emergency()`
- [ ] Write unit tests for cooldown enforcement
- [ ] Write unit tests for amount limit
- [ ] Write integration tests for rapid transfer attempts
- [ ] Add events for rate limit violations
- [ ] Update documentation

## Testing Requirements

- Test normal emergency transfer (within cooldown)
- Test rapid transfer attempts are blocked
- Test cooldown expiration allows next transfer
- Test amount limit enforcement
- Test emergency config updates
- Test multiple transfers over time

## Configuration Recommendations

- **Cooldown:** 3600 seconds (1 hour) minimum
- **Max Amount:** 10% of total wallet balance per transfer
- **Daily Limit:** Consider adding daily transfer count limit

## Estimated Effort

2-3 days

## Related Issues

- Relates to THREAT_MODEL.md Section 3.4
- Blocks mainnet deployment
