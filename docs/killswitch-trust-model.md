# Emergency Killswitch: Trust Model

## Overview

The `emergency_killswitch` contract uses a **single-admin model** with no roles,
no multi-sig baked into the contract itself, and no secondary guardians. All
state-mutating operations are gated by `admin.require_auth()`. Read-only queries
(`is_paused`, `list_paused_functions`, `is_module_paused`, `is_function_paused`,
`get_unpause_schedule`) require no authentication — pause state is observable
on-chain by design.

---

## Who Can Trigger (Pause)

| Action | Authorized Caller | Notes |
|--------|-------------------|-------|
| `pause()` | **Admin only** | Immediate. Also clears any pending `UnpauseSchedule`. |
| `pause_module(module_id)` | **Admin only** | Pauses every function in the named module. |
| `pause_function(module_id, func)` | **Admin only** | Pauses a single function. Capped at 10 per module. |

`initialize(admin)` has **no auth check** — it is guarded only by the
`AlreadyInitialized` error. The deployer must call it immediately after
deployment; otherwise anyone could re-initialize with a different admin.

---

## Who Can Clear (Unpause / Recover)

| Action | Authorized Caller | Prerequisites |
|--------|-------------------|---------------|
| `unpause()` | **Admin only** | Requires a valid `UnpauseSchedule` whose timestamp has been reached. |
| `clear_emergency_state()` | **Admin only** | **No** schedule required. Bypasses timelock. Admin recovery escape hatch. |
| `unpause_module(module_id)` | **Admin only** | None. |
| `unpause_function(module_id, func)` | **Admin only** | None. |

### Key Distinction: `unpause` vs `clear_emergency_state`

- **`unpause`** — the standard recovery path. Requires a pre-scheduled timelock
  via `schedule_unpause(time)` and a ledger timestamp >= that time. Designed for
  planned recovery after an incident.
- **`clear_emergency_state`** — the break-glass path. Clears `GlobalPaused` and
  `UnpauseSchedule` immediately, **bypassing the timelock**. Exists to recover
  from the stuck-paused state created when `pause()` is called after
  `schedule_unpause()` (which wipes the schedule), leaving `unpause()` unusable.

Both paths require admin authorization.

---

## What State Is Preserved

### `clear_emergency_state()` — the recovery escape hatch

| State Variable | Preserved? |
|----------------|------------|
| `Admin` | ✅ Preserved |
| `ModulePaused(module_id)` for every module | ✅ Preserved |
| `PausedFunctions(module_id)` for every module | ✅ Preserved |
| `GlobalPaused` | ❌ Cleared (set to `false`) |
| `UnpauseSchedule` | ❌ Cleared (removed) |

Module- and function-level pauses survive `clear_emergency_state()` by design.
This allows the admin to restore global operations while keeping specific
modules or functions paused until those incidents are individually resolved.

### `unpause()` — the standard recovery path

| State Variable | Preserved? |
|----------------|------------|
| `Admin` | ✅ Preserved |
| `ModulePaused(module_id)` for every module | ✅ Preserved |
| `PausedFunctions(module_id)` for every module | ✅ Preserved |
| `GlobalPaused` | ❌ Cleared (set to `false`) |
| `UnpauseSchedule` | ❌ Cleared (removed) |

Same state preservation as `clear_emergency_state()`, but gated by the
timelock schedule.

### `pause()` — global emergency stop

| State Variable | Preserved? |
|----------------|------------|
| `Admin` | ✅ Preserved |
| `ModulePaused(module_id)` for every module | ✅ Preserved |
| `PausedFunctions(module_id)` for every module | ✅ Preserved |
| `GlobalPaused` | ❌ Set to `true` |
| `UnpauseSchedule` | ❌ Cleared (removed) |

Calling `pause()` while a scheduled unpause is pending cancels it. This
prevents a stale schedule from reactivating the system before the incident
is resolved.

---

## Admin Transfer

There is a single `DataKey::Admin` address. Transfers follow a simple
current-admin-authorizes flow:

1. Current admin calls `transfer_admin(new_admin)`.
2. `new_admin` must not be the contract's own address (prevents bricking).
3. On success, `DataKey::Admin` is overwritten. The previous admin loses all
   authority immediately.
4. The event `AdminTransferred { old_admin, new_admin, timestamp }` is emitted.

There is no multi-step handover, no pending-admin pattern, and no way to
recover a lost admin once transferred.

---

## Security Properties

1. **Single point of trust**: The admin address is the sole gatekeeper. If the
   admin key is compromised, the attacker can pause, unpause, schedule unpauses,
   transfer admin, and clear emergency state.
2. **No bricking**: Both `initialize` and `transfer_admin` reject the contract's
   own address as admin, ensuring there is always a reachable recovery path.
3. **Timelock for `unpause`**: The standard unpause path requires a future-dated
   schedule. This prevents rapid toggle (oscillation) attacks and premature
   reactivation.
4. **`clear_emergency_state` as escape hatch**: The timelock can leave the
   contract stuck-paused (e.g., after a re-pause that wiped the schedule).
   `clear_emergency_state` exists specifically to remedy this — but it is a
   deliberate trade-off that bypasses the cooling-off window.
5. **Observable state**: All pause state is readable without authentication.
   Monitoring systems can independently verify the system's pause status.
6. **External trust assumption**: The admin is expected to be a secure multi-sig
   or hardware-backed account. The contract itself does not enforce this.

---

## Invariant Summary

```
                ┌──────────────┐
                │  initialize  │  (no auth — deployer must call immediately)
                └──────┬───────┘
                       ▼
              ┌────────────────┐
              │    Active      │
              └───┬────────┬──┘
                  │        │
         pause()  │        │  clear_emergency_state()
          (admin) │        │  (admin, bypasses timelock)
                  ▼        │
          ┌───────────┐    │
          │  Paused   │◄───┘
          └───┬───────┘
              │
    schedule_unpause(time)  (admin, time >= now)
              │
              ▼
      ┌───────────────┐
      │   Scheduled   │  unpause() succeeds when ledger >= time (admin)
      └───────┬───────┘
              │
         unpause()  or  clear_emergency_state()
              │
              ▼
          ┌────────┐
          │ Active │
          └────────┘
```

Module-level (`pause_module` / `unpause_module`) and function-level
(`pause_function` / `unpause_function`) pauses operate orthogonally to the
global state machine above. Module and function pauses are **not** cleared by
`unpause()` or `clear_emergency_state()`.
