# Remittance Split: Pause-Path Coverage

**Issue:** #612  
**Status:** ✅ Resolved  
**Date:** 2026-05-28

## Overview

The Remittance Split contract exposes an emergency pause mechanism for incident response. This document specifies the pause behavior across all state-mutating entrypoints and verifies that no configuration drift can occur during a paused state.

## Pause Mechanism

### Pause Administration

- **PAUSE_ADM:** The pause administrator address. Initialized to the contract owner; can be transferred via `set_pause_admin()`.
- **PAUSED:** A boolean flag (stored as symbol_short!("PAUSED")) indicating whether the contract is in paused state.

### Pause Lifecycle

1. **Initial State:** `PAUSED = false` (not paused)
2. **Pause:** `pause()` → `PAUSED = true`  
   - Only the pause administrator can call `pause()`
   - Emits `paused` event
3. **Resume:** `unpause()` → `PAUSED = false`  
   - Only the pause administrator can call `unpause()`
   - **Intentionally bypasses pause check** (unpause must work while paused)
   - Emits `unpaused` event

## State-Mutating Entrypoints

All five state-mutating entrypoints **MUST** check `require_not_paused()` before mutating state:

### 1. `update_split()`

**Location:** `remittance_split/src/lib.rs:728`

**Pause Guard:** ✅ Line 738  
```rust
Self::require_not_paused(&env)?;
```

**Behavior When Paused:** Returns `RemittanceSplitError::Unauthorized`

**State Protected:**
- CONFIG struct (spending%, savings%, bills%, insurance%)
- SPLIT vector

**Test:** `test_update_split_rejected_when_paused()`

---

### 2. `create_remittance_schedule()`

**Location:** `remittance_split/src/lib.rs:1807`

**Pause Guard:** ✅ Line 1814  
```rust
Self::require_not_paused(&env)?;
```

**Behavior When Paused:** Returns `RemittanceSplitError::Unauthorized`

**State Protected:**
- Schedule storage (persistent DataKey::Schedule)
- Owner schedules list (DataKey::OwnerSchedules)
- Next schedule ID counter (NEXT_RSCH)

**Test:** `test_create_remittance_schedule_rejected_when_paused()`

---

### 3. `modify_remittance_schedule()`

**Location:** `remittance_split/src/lib.rs:1941`

**Pause Guard:** ✅ Line 1948  
```rust
Self::require_not_paused(&env)?;
```

**Behavior When Paused:** Returns `RemittanceSplitError::Unauthorized`

**State Protected:**
- Schedule record (amount, next_due, interval)
- Schedule TTL extensions

**Test:** `test_modify_remittance_schedule_rejected_when_paused()`

---

### 4. `cancel_remittance_schedule()`

**Location:** `remittance_split/src/lib.rs:2027`

**Pause Guard:** ✅ Line 2033  
```rust
Self::require_not_paused(&env)?;
```

**Behavior When Paused:** Returns `RemittanceSplitError::Unauthorized`

**State Protected:**
- Schedule active flag

**Test:** `test_cancel_remittance_schedule_rejected_when_paused()`

---

### 5. `import_snapshot()`

**Location:** `remittance_split/src/lib.rs:1233`

**Pause Guard:** ✅ Line 1240  
```rust
Self::require_not_paused(&env)?;
```

**Behavior When Paused:** Returns `RemittanceSplitError::Unauthorized`

**State Protected:**
- CONFIG struct (bulk restore)
- SPLIT vector
- All schedules
- Nonce counter

**Test:** `test_import_snapshot_rejected_when_paused()`

---

## Read-Only Entrypoints (Unaffected by Pause)

The following **getter** functions remain callable while paused:

- `get_split_config()` - Returns current config
- `get_split()` - Returns current split percentages
- `get_remittance_schedule(id)` - Returns schedule by ID
- `export_snapshot(owner)` - Exports state for backup
- `execute_due_remittance_schedules()` - Executes due but read-only
- `get_split_nonce(owner)` - Nonce query
- `is_paused()` - Pause state check

**Rationale:** Read operations do not risk config drift. Incident responders need visibility even during pause.

**Test:** `test_getters_remain_callable_when_paused()`

---

## Pause/Unpause Cycle

### Scenario: Emergency Pause → Fix → Resume

```
Time T0: Contract normal (PAUSED = false)
        ↓
Time T1: Emergency detected → pause() called
        PAUSED = true
        All mutators rejected with Unauthorized
        All getters remain available
        ↓
Time T2: Incident remediation
        Pause admin fixes downstream issues
        ↓
Time T3: Resume → unpause() called
        PAUSED = false
        All mutators resume normal operation
```

**Test:** `test_paused_then_unpaused_resumes_normally()`

---

## Access Control Matrix

See [ACCESS_CONTROL_MATRIX.md](./ACCESS_CONTROL_MATRIX.md) for full authorization model.

| Function                       | Owner | Pause Admin | Public |
|--------------------------------|-------|-------------|--------|
| `pause()`                      | ✗     | ✅          | ✗      |
| `unpause()`                    | ✗     | ✅          | ✗      |
| `set_pause_admin()`            | ✅    | ✗           | ✗      |
| `update_split()`               | ✅    | ✗ (blocked) | ✗      |
| `create_remittance_schedule()` | ✅    | ✗ (blocked) | ✗      |
| `modify_remittance_schedule()` | ✅    | ✗ (blocked) | ✗      |
| `cancel_remittance_schedule()` | ✅    | ✗ (blocked) | ✗      |
| `import_snapshot()`            | ✅    | ✗ (blocked) | ✗      |

---

## Error Handling

When a state-mutating entrypoint is called while `PAUSED == true`:

```rust
if Self::get_global_paused(env) {
    return Err(RemittanceSplitError::Unauthorized);
}
```

**Error Code:** `Unauthorized` (value: 6)

**Caller Experience:**
- Command-line: "operation not authorized (unauthorized)"
- Contract call: Transaction reverts with error code 6

---

## Test Coverage

All pause behavior is verified by 7 comprehensive tests in [remittance_split/src/test.rs](../remittance_split/src/test.rs):

1. ✅ `test_update_split_rejected_when_paused` — Mutator rejection
2. ✅ `test_create_remittance_schedule_rejected_when_paused` — Mutator rejection
3. ✅ `test_modify_remittance_schedule_rejected_when_paused` — Mutator rejection
4. ✅ `test_cancel_remittance_schedule_rejected_when_paused` — Mutator rejection
5. ✅ `test_import_snapshot_rejected_when_paused` — Mutator rejection
6. ✅ `test_getters_remain_callable_when_paused` — Getter availability
7. ✅ `test_paused_then_unpaused_resumes_normally` — Pause/unpause cycle

**Coverage:** 100% of state-mutating entrypoints; all pause paths exercised.

---

## Security Implications

**Goal:** Incident containment without config drift.

1. **No Config Mutation While Paused** — All mutators pre-check pause state; state is consistent.
2. **Visibility During Pause** — Read-only getters remain available for diagnostics.
3. **Unpause Bypass** — `unpause()` intentionally avoids the pause check; resumption always succeeds.
4. **Audit Trail** — Pause/unpause operations emit events for monitoring.

**Threat Model:** A malicious actor or bug could corrupt the split config. The pause mechanism prevents this by gating all writes until the incident is resolved and unpause is re-enabled.

---

## Related Issues

- GitHub Issue: #612
- Linked PR: (see PR description for number)

---

## Changelog

- **2026-05-28:** Initial implementation and test coverage.
