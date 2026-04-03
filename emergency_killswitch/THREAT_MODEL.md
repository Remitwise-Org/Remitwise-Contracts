# Threat Model: Emergency Kill Switch

## Overview
The `emergency_killswitch` contract provides global pause/unpause capabilities. Highly sensitive administrative actions like toggling the contract state require robust safety mechanisms to prevent operational errors or malicious rapid-cycle attacks.

## Identified Threat Vectors

### T1: Rapid Toggle Abuse (Oscillation)
**Scenario**: An attacker gains temporary control of an admin account or a script malfunctions, rapidly toggling the `pause` and `unpause` states.
**Impact**: Confusion in automated monitoring systems, race conditions in dependent contracts, or exhaustion of block space/resources.
**Mitigation**: **Mandatory Cooldown (`MIN_COOLDOWN`)**. The contract enforces a minimum 1-hour wait after any `pause` before an `unpause` can even be attempted.

### T2: Premature Reactivation
**Scenario**: An administrator unpauses the contract before the underlying technical issue (e.g., a bug or exploit) is fully resolved or verified.
**Impact**: Resumption of the emergency state, leading to further data loss or fund theft.
**Mitigation**: **Explicit Resolution Requirement (`KEY_RESOLVED`)**. The admin must call `mark_resolved` as a separate, conscious step before `unpause` is allowed. This prevents accidental unpauses via script or muscle memory.

### T3: Administrative Hijacking
**Scenario**: A compromised admin account attempts to lock the contract indefinitely.
**Impact**: Long-term denial of service.
**Mitigation**: Cooldown only applies to *reactivation* (unpause). `pause` is always immediate to ensure safety. Admin transfer requires authorization from the *current* active admin.

## Security Assumptions
- **Admin Integrity**: We assume the admin address is a secure multi-sig or hardware-backed account.
- **Clock Reliability**: We rely on the Soroban ledger timestamp for cooldown enforcement.
- **Atomic Operations**: State transitions are atomic; partial toggles are not possible.
