# Emergency and Administrator Controls: Concurrency and Race Safety Design & Specification (QE-2026-08)

## Executive Summary
This document specifies the deterministic concurrency, conflict resolution, and race safety guarantees for emergency controls, quorum activations, and administrator rotation in `emergency_killswitch` (Issue #1760).

During high-stress security incidents, concurrent administrative or automated tooling operations must not produce impossible states, lift emergency pauses prematurely, or create race windows.

---

## Key Invariants & Design

### 1. Optimistic Concurrency Control (OCC) for Signer Rotation
- **Function**: `configure_signers_with_epoch(env, caller, expected_epoch, signers, threshold)`
- **Invariant**: Any modification to the signer set requires matching `expected_epoch == get_signer_epoch()`.
- **Conflict Behavior**: If two concurrent transactions attempt to rotate signers, the second call fails with `Error::EpochMismatch` and leaves state untouched.

### 2. Administrator Rotation Epoch (`KillSwitchEpoch`)
- **Functions**: `transfer_admin(env, new_admin, ep)`, `bump_kill_switch_epoch(env, caller)`
- **Invariant**: `transfer_admin` requires `ep == get_kill_switch_epoch()`.
- **Conflict Behavior**: Calling `bump_kill_switch_epoch` or a prior `transfer_admin` increments `KillSwitchEpoch`, invalidating any in-flight or replayed `transfer_admin` call with `Error::EpochMismatch`.

### 3. Threshold Activation & Recovery Serialization
- **Functions**: `activate(env, epoch, approvals, scope)`, `recover(env, epoch, approvals)`
- **Invariant**:
  - `activate` requires `epoch == get_signer_epoch()` and `!has(&DataKey::ActivationEpoch)`.
  - Concurrent `activate` attempts return `Error::ActivationAlreadyActive`.
  - `recover` requires `epoch == active_epoch == get_signer_epoch()` and `timestamp >= RecoveryReadyAt`.
  - Repeated `recover` calls return `Error::NotActive`.

### 4. Admin Pause & Threshold Activation Scope Synchronization
- **Functions**: `pause`, `pause_with_reason`, `pause_module`, `pause_function`, `recover`
- **Invariant**: If an administrator issues an emergency pause (`pause`, `pause_module`, `pause_function`) while a threshold activation is active for a matching scope, the contract updates `DataKey::ScopeWasPaused = true`.
- **Safety Guarantee**: When threshold `recover()` runs, `ScopeWasPaused` being `true` guarantees that `recover()` will **not** clear the administrator's active emergency pause.

### 5. Valid State Machine Transitions
- **Functions**: `schedule_unpause(env, time)`, `unpause(env)`
- **Invariant**: Calling `schedule_unpause` or `unpause` requires `is_paused() == true`.
- **Conflict Behavior**: Calling `schedule_unpause` or `unpause` on an unpaused contract fails deterministically with `Error::NotActive`.

---

## Conflict and Retry Matrix

| Operation | Concurrent Condition | Action Result | Final Contract State | Client Retry Contract |
| --- | --- | --- | --- | --- |
| `configure_signers_with_epoch` | `expected_epoch != current` | `Err(Error::EpochMismatch)` | Unchanged (epoch retained) | Refresh `get_signer_epoch()`, collect new signature if required, retry with new epoch. |
| `activate` | Active activation present | `Err(Error::ActivationAlreadyActive)` | Existing scope retained | Do NOT retry. Query `get_pause_state()` to inspect active incident. |
| `activate` | Signer set rotated in-flight | `Err(Error::EpochMismatch)` | Unchanged | Refresh `get_signer_epoch()`, collect fresh quorum approvals, retry. |
| `recover` | `recover` already executed | `Err(Error::NotActive)` | Unchanged (unpaused) | Do NOT retry. Activation state has already been cleared. |
| `recover` | Signer set rotated during activation | `Err(Error::EpochMismatch)` | Pause retained | Collect new quorum approvals under the updated `SignerEpoch`, retry `recover`. |
| `transfer_admin` | Epoch bumped concurrently | `Err(Error::EpochMismatch)` | Admin unchanged | Refresh `get_kill_switch_epoch()`, update payload epoch, retry. |
| `schedule_unpause` / `unpause` | Contract is unpaused (`!is_paused()`) | `Err(Error::NotActive)` | Unchanged | Do NOT retry. Contract is already in unpaused state. |

---

## Verification & Rollback Analysis

### Zero Partial State Assertion
Soroban transactional atomicity guarantees that any transaction returning `Err(Error)` rolls back all storage changes. Tests in `emergency_killswitch/tests/test_killswitch.rs` explicitly verify that rejected operations leave instance storage keys untouched.

### Snapshot Upgrade Safety
The pre-upgrade snapshot and restore mechanism (`pre_upgrade` / `restore_from_snapshot`) captures `kill_switch_epoch`, `signer_epoch`, `signer_threshold`, `activation_epoch`, `active_scope`, `recovery_ready_at`, and `scope_was_paused`. Restoring from snapshot restores exact epoch and scope markers atomically.
