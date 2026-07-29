# Threat Model: Emergency Kill Switch

## Overview
The `emergency_killswitch` contract provides global pause/unpause capabilities. Highly sensitive administrative actions like toggling the contract state require robust safety mechanisms to prevent operational errors or malicious rapid-cycle attacks.

## Identified Threat Vectors

### T1: Rapid Toggle Abuse (Oscillation) & Premature Reactivation
**Scenario**: An attacker gains temporary control of an admin account or a script malfunctions, rapidly toggling the `pause` and `unpause` states. Alternatively, an administrator unpauses the contract prematurely before the underlying technical issue (e.g., a bug or exploit) is fully resolved or verified.
**Impact**: Confusion in automated monitoring systems, race conditions in dependent contracts, or premature resumption of the vulnerable state, leading to further data loss or fund theft.
**Mitigation**: **Unpause Timelock Invariant**.
- The contract enforces a mandatory cooling-off window. The global pause state cannot be lifted immediately via `unpause`. Instead, an unpause must be scheduled in advance by calling `schedule_unpause` with a future timestamp, recording the unpause time.
- The `unpause` function enforces that `env.ledger().timestamp() >= scheduled_time` (recorded in `DataKey::UnpauseSchedule`). This check cannot be bypassed, and the state returned by `is_paused` remains `true` until `unpause` is successfully called.
- The timelock cannot be bypassed by re-calling `schedule_unpause` with a past timestamp, as the contract explicitly rejects any past-dated schedules (`time < env.ledger().timestamp()`).
- To prevent stale or queued schedules from being used to immediately lift a future pause, any new call to `pause` automatically cancels and removes any pending schedule stored under `DataKey::UnpauseSchedule`.

### T2: Administrative Hijacking
**Scenario**: A compromised admin account attempts to lock the contract indefinitely.
**Impact**: Long-term denial of service.
**Mitigation**: The timelock and scheduling requirement only apply to reactivation (`unpause`). The emergency `pause` function is always immediate to ensure that the system can be secured instantly during an incident without delay. Admin transfer requires authorization from the current active admin.

## Security Assumptions
- **Admin Integrity**: We assume the admin address is a secure multi-sig or hardware-backed account.
- **Clock Reliability**: We rely on the Soroban ledger timestamp for cooldown/timelock enforcement.
- **Atomic Operations**: State transitions are atomic; partial toggles are not possible.
- **Storage Persistence**: State keys represented by `DataKey` are persisted correctly across ledger updates.
