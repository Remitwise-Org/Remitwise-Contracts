# Migration Flags — Operator Runbook

**Audience: operators** running snapshot-import scripts, managing investigation
freezes, or troubleshooting blocked writes.

This document covers every boolean flag or gate that controls whether writes
are allowed on a Remitwise contract during data migration or a security
investigation. Each flag lives in contract storage (or a `MigrationTracker`
struct); none are compile-time features.

---

## 1. MigrationTracker::completed (write-completion flag)

**Declared in:** `data_migration/src/lib.rs:649`

```rust
pub struct MigrationTracker {
    imported_payloads: HashMap<(String, u32), u64>,
    pub completed: bool,          // <-- the flag
}
```

### Meaning

`completed = true` means "the operator has finished importing all snapshots;
it is now safe to allow live writes."  Until this flag is set, the function
[`verify_migration_completed`] at `data_migration/src/lib.rs:737` returns
`Err(MigrationError::MigrationNotCompleted)`, blocking every write entry
point that calls it.

### When it is set and cleared

- **Set** — explicitly by the operator calling `tracker.mark_completed()`.
  ```rust
  tracker.mark_completed();
  ```
- **Cleared** — never. A completed migration stays completed. Once `true`,
  the flag is irreversible for that `MigrationTracker` instance. (If the
  operator needs to re-import, they must create a fresh tracker.)

### What happens if it is missing / stale / misused

| Scenario | Behaviour | Risk |
|----------|-----------|------|
| Operator forgets to call `mark_completed()` after the last import | Every write entrypoint that calls `verify_migration_completed` returns `MigrationNotCompleted`. All live writes are rejected. | **Medium.** Safe-fail — no data corruption, but the contract refuses to serve users until the operator calls `mark_completed()`. |
| Operator calls `mark_completed()` before all snapshots are imported | Writes are unblocked while the on-chain state is still partially migrated. A live write could land on top of an incomplete record. | **High.** Silent data corruption. The `completed` flag has **no** coupling to the actual import count; it is a pure operator promise. |
| Entrypoint does not call `verify_migration_completed` at all | Migration write gates are skipped entirely for that path. | **High.** Defence-in-depth hole (see issue #845). |

### Check it yourself (read-only, no state mutation)

```rust
use data_migration::MigrationTracker;

// If you hold a reference to the tracker:
let ok = tracker.is_completed();
```

For on-chain storage the `MigrationTracker` is persisted via
`env.storage().instance().get()` — the exact key is contract-specific.

---

## 2. MigrationTracker::imported_payloads (replay-protection set)

**Declared in:** `data_migration/src/lib.rs:645`

```rust
imported_payloads: HashMap<(String, u32), u64>,
//                    (checksum, version)   timestamp_ms
```

### Meaning

This is a **set of flags** — one entry per snapshot that has already been
imported through this tracker. The key is `(checksum, version)`.  Each entry
records the Unix-millis timestamp when the import happened.

### When entries are added and removed

- **Added** — by `tracker.mark_imported(&snapshot, timestamp_ms)` at
  `data_migration/src/lib.rs:679`.  Every tracked import function
  (`import_from_json`, `import_from_binary`) calls this automatically.
- **Removed** — only by `tracker.unmark_imported_by_identity(checksum, version)`
  at line 702, which is called during rollback
  (`RollbackMetadata::restore`).

### What happens if it is missing / stale / misused

| Scenario | Behaviour | Risk |
|----------|-----------|------|
| Same snapshot imported twice (tracked path) | `mark_imported` detects the duplicate key and returns `Err(MigrationError::DuplicateImport)`. | **None.** Correctly rejected. |
| Same snapshot imported twice (untracked path — `import_from_json_untracked` / `import_from_binary_untracked`) | A throwaway `MigrationTracker` is created per call; the second import succeeds. | **High.** Double-applied state. The doc comment at line 1064 calls this a "footgun" for exactly this reason. |
| Tracker is persisted across contract calls but lost (e.g. contract upgrade clears the storage key) | Re-import of any previously-seen snapshot succeeds silently, producing duplicate state. | **High.** Replay attack vector. |
| `unmark_imported_by_identity` is called outside rollback | The replay-protection entry for that payload is removed. A subsequent import of the same snapshot would succeed. | **Medium.** This function is `pub` but is only intended for rollback. Currently only called from `RollbackMetadata::restore`. |

### Check the set (read-only)

```rust
if tracker.is_imported(&snapshot) {
    // already applied
}
```

(Not yet exposed as a public method — the field is `pub(crate)`.)

---

## 3. Investigation-epoch flag (write-freeze during incident response)

**Declared in:** `remitwise-common/src/lib.rs:2324–2408`

```rust
pub(crate) const STORAGE_INVESTIGATION_EPOCH: Symbol = symbol_short!("INVEST_EPOCH");
```

Storage: instance entry `INVEST_EPOCH` → `u64` (ledger timestamp of epoch end,
or absent/0 when no epoch is active).

### Meaning

When the stored `end_time > env.ledger().timestamp()`, writes are frozen.
The gate function [`require_no_investigation_epoch`] at
`remitwise-common/src/lib.rs:2371` returns
`Err(InvestigationEpochError::WriteBlocked)`.

### When it is set and cleared

- **Set** — by calling `start_investigation_epoch(env, duration_secs)`.
  The contract on its own does **not** authenticate this call; the calling
  contract's entrypoint must first call `admin.require_auth()`.
  ```rust
  // Inside an admin-guarded entrypoint:
  admin.require_auth();
  start_investigation_epoch(&env, 3600); // freeze for 1 hour
  ```
- **Cleared explicitly** — by calling `clear_investigation_epoch(&env)`.
  Same authentication caveat applies.
- **Expires automatically** — when `env.ledger().timestamp()` passes the
  stored `end_time`. No operator action needed.

### What happens if it is missing / stale / misused

| Scenario | Behaviour | Risk |
|----------|-----------|------|
| Investigation epoch started but never cleared | Self-clearing after `duration_secs` elapses (ledger-time comparison). | **Low.** Writes resume automatically; no stuck state. |
| `start_investigation_epoch` called without admin auth | Auth is **not** enforced by the function itself. An attacker could freeze the contract. | **Critical.** The calling entrypoint **must** call `require_auth()` before `start_investigation_epoch`. This is documented but not compiler-enforced. |
| Entrypoint omits `require_no_investigation_epoch` | Writes proceed during an active investigation, potentially destroying evidence or letting an exploit continue. | **High.** Defence-in-depth gap. |
| Very large `duration_secs` (e.g. `u64::MAX`) | The epoch never expires; writes are frozen permanently (or until `clear_investigation_epoch` is called). | **Medium.** Requires a coordinated multi-sig admin call to clear. |
| `clear_investigation_epoch` called when no epoch is active | No-op (just removes a non-existent key). | **None.** Safe. |

### Check it yourself (read-only)

```rust
use remitwise_common::is_investigation_epoch_active;

if is_investigation_epoch_active(&env) {
    // investigation epoch is in effect
}
```

---

## 4. CONTRACT_VERSION (schema-compatibility gate)

**Declared in:** `remitwise-common/src/lib.rs:193`

```rust
pub const CONTRACT_VERSION: u32 = 1;
```

This is not a mutable flag but a **version constant** used by
`verify_config_migration` to reject writes from an out-of-date contract
instance.  The gate is:

```rust
fn verify_config_migration(version: u32) -> Result<(), MigrationError> {
    if version < CONTRACT_VERSION {
        Err(MigrationError::OutdatedVersion)
    } else {
        Ok(())
    }
}
```

> **⚠️ Note for operators:** `verify_config_migration` is referenced in
> `remitwise-common/src/tests.rs:1744` but is **not currently exported**
> from `remitwise-common/src/lib.rs`.  The test may be stale.  If a write
> entrypoint is supposed to call this gate but does not, a freshly-upgraded
> contract with old storage could mutate state before the operator has run
> the schema-migration entrypoint.  Verify the actual entrypoint code path
> before relying on this check.

### When the version is bumped

`CONTRACT_VERSION` is incremented manually by a contributor when a backward-
incompatible schema change is introduced.  The operator must then call the
contract's `migrate()` entrypoint before any other write operation.

---

## Related documents

- [`data_migration` crate docs](data_migration/src/lib.rs) — full module
  documentation with the import pipeline diagram.
- [`docs/MIGRATIONS.md`](MIGRATIONS.md) — on-chain struct upgrade rules
  (contributor-facing).
- [`docs/MIGRATION_PATHS.md`](MIGRATION_PATHS.md) — snapshot versioning,
  N-2 compatibility, and replay-protection details.
- [`docs/migration-import-safety.md`](migration-import-safety.md) — the
  complete import validation pipeline.
- [`docs/CROSS_CONTRACT_INVARIANTS.md`](CROSS_CONTRACT_INVARIANTS.md) —
  epoch guards and write-freeze invariants that span multiple contracts.
- Issue [#845](https://github.com/Remitwise-Org/Remitwise-Contracts/issues/845)
  — "Wire `verify_migration_completed` into write entrypoints" (tracking
  the coverage gap).

[`verify_migration_completed`]: ../data_migration/src/lib.rs#L737
[`require_no_investigation_epoch`]: ../remitwise-common/src/lib.rs#L2371
