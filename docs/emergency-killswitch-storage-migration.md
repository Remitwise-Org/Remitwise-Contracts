# Emergency Killswitch — Storage & Migration Compatibility

**Issue:** [#1763](https://github.com/Remitwise-Org/Remitwise-Contracts/issues/1763)  
**Status:** Implemented  
**Contract:** `emergency_killswitch`

---

## 1. Design Overview

The emergency-killswitch contract manages system-wide pause, per-module pause, per-function pause, threshold-based activation with recovery delay, and admin rotation.  After a contract WASM upgrade, existing ledger entries are not transformed — the new binary must deserialize old data correctly.

This change adds:

1. **Storage version tracking** — a canonical `StorageVersion` key so off-chain tooling and on-chain migration logic know which schema layout is active.
2. **Resumable, observable migration** — `migrate_storage` advances the schema one version at a time, recording progress so partial failures can be retried without data loss.
3. **Pre-upgrade snapshot & restore** — `pre_upgrade` captures the full emergency state; `restore_from_snapshot` rolls it back after a failed upgrade.
4. **Snapshot TTL** — snapshots expire after 24 hours to prevent stale restores.

---

## 2. New Storage Keys

| Key              | Type                | Symbol       | Purpose                                                  |
|------------------|---------------------|--------------|----------------------------------------------------------|
| `StorageVersion` | `u32`               | `STOR_VER`   | Current storage schema version (0 = pre-versioning)       |
| `Snapshot`       | `EmergencyStateSnapshot` | `SNAP`  | Full emergency state captured before upgrade              |
| `SnapshotTimestamp` | `u64`            | `SNAP_TS`    | Timestamp when snapshot was taken (for TTL enforcement)   |
| `MigrationProgress` | `MigrationProgress` | `MIGRPRG`  | Resumable migration state                                |

All keys follow the 9-character `symbol_short!` convention.

---

## 3. Migration Protocol

### 3.1 Versioning Scheme

- `STORAGE_VERSION` (compile-time constant) defines the target schema version.
- `DataKey::StorageVersion` (on-chain) records the version currently active.
- Version 0 = legacy deployment that predates this tracking.
- Each `migrate_storage` call advances exactly one version number.

### 3.2 Resumability

`MigrationProgress` records:
- `from_version` — version before migration started
- `to_version` — target version
- `completed_step` — number of migration steps finished
- `total_steps` — total steps needed
- `last_run_at` — timestamp of last step

If a transaction is submitted but the execution halts (e.g. out of gas), the next `migrate_storage` call reads the progress and resumes from the next uncompleted step.  Each step is atomic — if a step panics, no partial state from that step is committed.

### 3.3 Idempotency

- `migrate_storage` when `current >= STORAGE_VERSION` → `Error::AlreadyMigrated`
- `pre_upgrade` overwrites any existing snapshot (last-writer-wins)
- `restore_from_snapshot` consumes the snapshot (removes from storage)

### 3.4 Events

| Event          | Topic                      | Payload                                    |
|----------------|----------------------------|--------------------------------------------|
| `migr_step`    | `(emergency, migr_step)`   | `(from_ver, to_ver, step, total)`          |
| `migr_done`    | `(emergency, migr_done)`   | `(new_version, timestamp)`                 |
| `snap_pre`     | `(emergency, snap_pre)`    | `(schema_version, timestamp)`              |
| `snap_rst`     | `(emergency, snap_rst)`    | `(schema_version, timestamp)`              |
| `snap_dsc`     | `(emergency, snap_dsc)`    | `(timestamp,)`                             |

---

## 4. Snapshot Protocol

### 4.1 Capture (`pre_upgrade`)

- Requires admin auth.
- Captures every `DataKey` value into an `EmergencyStateSnapshot` struct.
- All optional fields in the snapshot use `Option<T>` for forward compatibility — newer snapshot fields added in future versions won't break deserialization of older payloads.
- Stored under `DataKey::Snapshot` with a timestamp under `DataKey::SnapshotTimestamp`.
- Valid for `SNAPSHOT_TTL` (24 hours).

### 4.2 Restore (`restore_from_snapshot`)

- Requires admin auth.
- Validates snapshot existence, TTL, and schema compatibility (`schema_version <= STORAGE_VERSION`).
- Restores each field individually — `Some` values are written, `None` values remove the key (clean state).
- Consumes the snapshot after successful restore (removes from storage).
- On failure (expired, not found), existing state is untouched — no partial writes.

### 4.3 Discard (`discard_snapshot`)

- Requires admin auth.
- Removes snapshot and timestamp from storage.
- Emits `snap_dsc` event.

---

## 5. Failure Behavior & State Safety

| Scenario                              | Behavior                                            |
|---------------------------------------|-----------------------------------------------------|
| `migrate_storage` called twice        | Second call returns `AlreadyMigrated` (no-op)       |
| `migrate_storage` with partial progress | Resumes from last completed step                  |
| `restore_from_snapshot` with expired snapshot | Returns `SnapshotExpired`, state untouched    |
| `restore_from_snapshot` without snapshot | Returns `SnapshotNotFound`, state untouched      |
| `pre_upgrade` called twice            | Overwrites existing snapshot (last-writer-wins)     |
| Non-admin calls to any entrypoint     | Returns `Unauthorized`, no state changed            |
| `discard_snapshot` when no snapshot exists | Silently succeeds (no-op)                     |

---

## 6. Forward & Backward Compatibility

- **Forward:** New fields added to `EmergencyStateSnapshot` must use `Option<T>`.  Old snapshots missing these fields will deserialize as `None`, which is handled gracefully by the restore logic.
- **Backward:** `storage_version()` returns 0 for legacy deployments.  `migrate_storage` advances from 0 to the target version, so upgrading a legacy deployment is safe.
- **No breaking changes:** All existing public entrypoints retain their original signatures and behavior.  The new entrypoints (`storage_version`, `migrate_storage`, `pre_upgrade`, `restore_from_snapshot`, `discard_snapshot`) are additive.

---

## 7. Rollback Considerations

1. Before upgrading the WASM, call `pre_upgrade` to capture the current state.
2. Deploy the new WASM.
3. If the upgrade needs to be rolled back:
   - Deploy the old WASM again.
   - Call `restore_from_snapshot` to recover the pre-upgrade state.
4. If the snapshot has expired (>24h), a fresh `pre_upgrade` must be taken with the new WASM before rollback.

---

## 8. Operational Limitations

- Snapshots expire after 24 hours.  For long-running rollouts, re-capture snapshots periodically.
- `migrate_storage` advances one version per call.  For multi-version jumps, call repeatedly until `AlreadyMigrated`.
- Module-level and function-level pauses are preserved through snapshot/restore — only the global pause flag and its associated metadata are affected by `clear_emergency_state`.

---

## 9. Security Assumptions

- Admin authentication is required for all mutating entrypoints (`migrate_storage`, `pre_upgrade`, `restore_from_snapshot`, `discard_snapshot`).
- The snapshot is stored in instance storage (same trust boundary as all other contract state).
- The TTL window (24h) bounds the window during which a stale snapshot can be used for restore.
- Snapshot schema version is validated against `STORAGE_VERSION` to prevent restoring a snapshot from a future contract version.
