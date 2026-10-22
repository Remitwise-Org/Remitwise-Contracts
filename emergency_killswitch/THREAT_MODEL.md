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

### T3: Forged Signer Approvals (Unauthorised Activation/Recovery)
**Scenario**: An attacker reads the public signer set (stored in instance storage, exposed via `list_signers_page`, and visible in `configure_signers` arguments) and calls `activate` or `recover` with a list of configured signer addresses that meets the threshold, without those signers having authorised anything.
**Impact**: Anyone can freeze the protocol at will (griefing / DoS) via `activate`, or lift an emergency pause the real signers put in place via `recover` once `RECOVERY_DELAY` has passed.
**Mitigation**: **Threshold-Signer Authorisation**.
- An `Address` argument proves nothing until the host verifies an authorisation entry. `validate_approvals` therefore calls `require_auth_for_args` on every approval, binding each authorisation to the specific action.
- The authorised argument tuple is `(action, epoch, scope)` where `action` is `symbol_short!("activate")` or `symbol_short!("recover")`, `epoch` is the signer epoch, and `scope` is the `PauseScope` (for `recover`, the scope is the stored paused scope).
- Because the action, epoch, and scope are part of the signed payload, an approval for `activate(Global)` cannot be replayed as an approval for `recover`, nor for a different scope or epoch.
- An approval list containing any address that did not authorise the exact invocation is rejected, and no state changes are written.
- The signer epoch check remains, so rotating signers invalidates in-flight approvals.
- Admin-only entrypoints are unchanged and continue to require admin authorisation.

## Security Assumptions
- **Admin Integrity**: We assume the admin address is a secure multi-sig or hardware-backed account.
- **Clock Reliability**: We rely on the Soroban ledger timestamp for cooldown/timelock enforcement.
- **Atomic Operations**: State transitions are atomic; partial toggles are not possible.
- **Storage Persistence**: State keys represented by `DataKey` are persisted correctly across ledger updates.
- **Signer Authorisation**: We assume configured signers protect their keys and that the host correctly enforces `require_auth_for_args` entries. Signer addresses are public and confer no authority without a matching authorisation entry.
