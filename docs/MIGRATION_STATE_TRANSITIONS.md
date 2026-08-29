# Migration State-Transition Invariants

This document describes the `MigrationAttemptStatus` state machine enforced
by `MigrationTracker` in `data_migration/src/lib.rs`.

## Purpose

Payment and settlement data migrated across contract upgrades must not be lost,
duplicated, or left in a partially-applied state.  The state machine in
`MigrationTracker` is the mechanism that prevents these failure modes.

Before this design was introduced, the lifecycle was **implicitly enforced
through if-let/take patterns** spread across four methods.  This made it easy
for new code to violate the lifecycle without triggering a compile-time or
test-time error.  The current design centralises every legal transition in a
single function (`is_legal_transition`) and enforces it at every public entry
point.

---

## State Machine

```
                   ┌─────────────────────────────────────────────────┐
                   │  MigrationTracker active_attempt state machine  │
                   └─────────────────────────────────────────────────┘

    ┌──────┐  begin_import   ┌────────────┐  mark_imported  ┌───────────┐
    │ None │ ──────────────► │ InProgress │ ───────────────► │ Completed │
    └──────┘                 └────────────┘                  └───────────┘
       ▲                      │          │
       │                      │          │ fail_import
       │ begin_import         │          ▼
       │ (retry)        ┌─────▼─────┐  ┌────────┐
       └────────────────│ RolledBack│  │ Failed │
                        └───────────┘  └────────┘
                              │              │
                              └──────────────┘
                                  begin_import
                                  (retry)
```

### States

| State        | Meaning                                                 |
|--------------|---------------------------------------------------------|
| `None`       | No active attempt. The tracker is idle.                 |
| `InProgress` | An import is actively running; batches may be applied.  |
| `Completed`  | Import succeeded; payload is in `imported_payloads`.    |
| `Failed`     | Import was explicitly abandoned via `fail_import`.      |
| `RolledBack` | Import was reversed via `RollbackMetadata::restore`.    |

### Legal Transition Matrix

| From         | To           | Operation / trigger                  |
|--------------|--------------|--------------------------------------|
| `None`       | `InProgress` | `begin_import` — fresh start         |
| `Failed`     | `InProgress` | `begin_import` — retry after failure |
| `RolledBack` | `InProgress` | `begin_import` — retry after rollback|
| `InProgress` | `Completed`  | `mark_imported` — import succeeded   |
| `InProgress` | `Failed`     | `fail_import` — explicit failure     |
| `InProgress` | `RolledBack` | `RollbackMetadata::restore`          |

All other pairs are **illegal** and return
`MigrationError::IllegalStateTransition`.

---

## Implementation

### `is_legal_transition(from, to) -> bool`

The single source of truth for the legal transition matrix.  Every entry point
that changes `MigrationAttemptStatus` calls this function.  To add or remove
a legal transition, update only this function; all enforcement is automatic.

```rust
pub fn is_legal_transition(
    from: Option<MigrationAttemptStatus>,
    to: MigrationAttemptStatus,
) -> bool { ... }
```

### Entry-point guards

| Method                 | Guard added                                  |
|------------------------|----------------------------------------------|
| `begin_import`         | Checks `is_legal_transition(current, InProgress)`; special-cases `MigrationAlreadyInProgress` for the `InProgress → InProgress` path. |
| `record_progress`      | Checks `attempt.status == InProgress`; returns `IllegalStateTransition` otherwise. |
| `fail_import`          | Checks `current_status == InProgress` before consuming the attempt; returns `IllegalStateTransition` otherwise. |
| `mark_imported`        | Checks `active.status == InProgress` when an active attempt exists for the same identity; returns `IllegalStateTransition` otherwise. |

### Fast-path compatibility

`mark_imported` may be called without a preceding `begin_import` (the
**fast path** used by `import_from_json` / `import_from_binary`).  In this
case no `active_attempt` exists and a synthetic `Completed` history entry is
created atomically.  The state-machine guard only fires if an `active_attempt`
with a non-`InProgress` status exists, which cannot be caused by any existing
public API but could occur if internal invariants were violated by future code.

---

## Failure Behavior

### Rejected operations leave no partial state

All rejection paths return an error **before** mutating any observable state:

- `begin_import` rejects by returning early; `active_attempt` is not modified.
- `record_progress` rejects while holding `&mut` on the attempt; if the status
  check fails the early return prevents any field from being updated.
- `fail_import` reads `current_status` via `as_ref` first, then takes ownership
  only after the status check passes; if the check fails the attempt is never
  consumed.
- `mark_imported` checks `active.status` via `&self` borrow before inserting
  into `imported_payloads`; if the check fails, `imported_payloads` is not
  modified.

### Stale identity errors also leave no partial state

`StaleMigrationAttempt` is returned when the snapshot passed to an operation
does not match the identity of the active attempt.  In `fail_import` the
attempt is restored to `active_attempt` before returning; in `record_progress`
the immutable borrow prevents any mutation.

---

## Compatibility Impact

### Backward-compatible changes

- Adding `IllegalStateTransition` to `MigrationError` is additive; existing
  match arms that use a catch-all `_` pattern continue to compile.
- `is_legal_transition` is a new public function; existing code does not call
  it and is not affected.
- The `record_progress` and `fail_import` guards fire only when `active_attempt`
  holds a non-`InProgress` status.  This cannot be caused by any existing code
  path, so no existing test or integration is affected.

### Non-breaking behavior change in `mark_imported`

The guard in `mark_imported` fires only when an active attempt exists and its
status is not `InProgress`.  This is a **defense-in-depth guard** that does not
affect any existing call pattern:

- The **fast path** (no `active_attempt`) is unchanged.
- The **tracked path** (active attempt exists) was always `InProgress` by
  construction before this change; the guard adds an explicit assertion without
  changing which inputs succeed.

### Documented compatibility change in `begin_import`

Previously, `begin_import` only checked `active_attempt.is_some()` before
returning `MigrationAlreadyInProgress`.  Now it explicitly checks
`is_legal_transition(current, InProgress)`:

- `None → InProgress`: still permitted (same as before).
- `InProgress → InProgress`: still returns `MigrationAlreadyInProgress`
  (same error, same behavior).
- `Failed → InProgress`: now explicitly permitted (was already permitted
  because `take` of a `Failed` attempt in `fail_import` cleared
  `active_attempt`; the guard is now the authoritative check).
- `RolledBack → InProgress`: same rationale as `Failed`.
- `Completed → InProgress`: would require `active_attempt` to hold a
  `Completed` attempt, which no existing code causes; returns
  `IllegalStateTransition`.

---

## Migration / Rollback Considerations

### Snapshot reconciliation (gap-free guarantee)

`ExportSnapshot::validate_for_import` already enforces:

1. `reconciliation_report()` is called and its `gap_free` flag must be `true`.
2. Duplicate logical keys (e.g. two goals with the same `(owner, id)`) are
   rejected before import.

These checks are **not** relaxed by the state-machine changes.

### Rollback safety

`RollbackMetadata::restore` is the only path into `RolledBack` state.  After
restore:

1. `imported_payloads` no longer contains the attempted payload's identity
   (`unmark_imported_by_identity`), so a retry via `begin_import` is possible.
2. `attempt_history` gains an entry with `status = RolledBack`, providing a
   full audit trail.
3. `active_attempt` is cleared.

Double rollback is idempotent: calling `restore` a second time removes an
already-absent key (no-op) and tries to find the active attempt (which is
already `None`).

---

## Operational Limitations

- A tracker with an `InProgress` attempt cannot begin a second concurrent
  attempt.  Multi-payload migrations must complete, fail, or roll back the
  current attempt before starting the next one.
- History is unbounded; long-running migration scripts that fail and retry many
  times will accumulate history entries.  For very high retry counts (> 100),
  consider trimming old `Failed`/`RolledBack` entries from history after
  successful completion.
- `MigrationTracker` is not thread-safe; callers are responsible for
  serialising access.

---

## Security Assumptions

- **Replay protection** is enforced by `imported_payloads` (keyed on
  `(checksum, version)`).  The state-machine changes do not weaken this.
- **Checksum integrity** is enforced by `validate_for_import` which is called
  inside `begin_import`.  An attempt cannot begin for a tampered or corrupted
  snapshot.
- **Partial-state isolation**: all rejection paths return before any
  observable mutation, so a rejected operation cannot leave the tracker in an
  inconsistent state that could be exploited by a subsequent call.
- The `is_legal_transition` function is pure (no side effects) and covers all
  16 `(Option<MigrationAttemptStatus>, MigrationAttemptStatus)` pairs; its
  correctness is independently verified by exhaustive unit tests.

---

## Test Coverage

All state-transition invariants are tested in `data_migration/src/lib.rs` under
the `// STATE-TRANSITION INVARIANT TESTS` section.  Coverage includes:

| Category                 | What is tested                                               |
|--------------------------|--------------------------------------------------------------|
| `is_legal_transition`    | All 16 `(from, to)` pairs (6 legal + 10 illegal)            |
| Legal edge: None → IP    | `begin_import` succeeds and sets `InProgress`               |
| Legal edge: IP → Comp    | `mark_imported` after `begin_import` succeeds               |
| Legal edge: IP → Failed  | `fail_import` transitions to `Failed`                       |
| Legal edge: IP → RB      | `RollbackMetadata::restore` transitions to `RolledBack`     |
| Legal edge: Failed → IP  | `begin_import` retry after `fail_import` succeeds           |
| Legal edge: RB → IP      | `begin_import` retry after rollback succeeds                |
| Stale identity           | `record_progress` / `fail_import` with wrong snapshot       |
| Repeated import          | `mark_imported` twice returns `DuplicateImport`             |
| Skipped `begin_import`   | `fail_import` / `record_progress` without begin             |
| Out-of-order             | `mark_imported` while different attempt is `InProgress`     |
| Fast path                | `mark_imported` without `begin_import` (synthetic Completed)|
| Full lifecycle           | Fresh → InProgress → progress → Completed                   |
| Failure lifecycle        | InProgress → Failed → InProgress → Completed                |
| Rollback lifecycle       | InProgress → RolledBack → InProgress → Completed            |
| Partial-state isolation  | Rollback leaves no trace in `imported_payloads`             |
| Concurrent trackers      | Two independent trackers do not interfere                   |
| Progress monotonicity    | Regression and overflow in `record_progress`                |
| Error Display            | `IllegalStateTransition` message contains from/to           |
