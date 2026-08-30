# Storage Layout Reference

This document describes on-chain storage keys and value shapes for every workspace contract:

- `remittance_split`
- `savings_goals`
- `bill_payments`
- `insurance`
- `family_wallet`
- `reporting`
- `orchestrator`
- `data_migration`

Scope: current implementation in this repository, focused on auditability and migration planning.

## Storage Key Naming Conventions

Contract documentation uses `Storage` consistently for Soroban instance and
persistent storage. `Store` is reserved for describing an action (for example,
storing a report), not as the name of a storage abstraction or key namespace.

All storage keys follow strict naming conventions to ensure consistency and compatibility with Soroban's `symbol_short!` macro:

- **Maximum length:** 9 characters (enforced by `symbol_short!`)
- **Format:** UPPERCASE_WITH_UNDERSCORES
- **Valid characters:** A-Z, 0-9, \_ (underscore)

These conventions are automatically validated in CI by three complementary test suites: a hand-maintained catalogue check (`storage_key_naming_test.rs`), a live source scan (`storage_key_source_scan_test.rs`) that parses each contract's `src/lib.rs` directly so a key added or renamed in code can't silently drift out of sync with the documented conventions, and a reserved-key check (`reserved_storage_keys_test.rs`) that fails the build if a contract reuses a key set aside for a future roadmap feature. See [Storage Key Naming Conventions](docs/storage-key-naming-conventions.md) for detailed guidelines, [Reserved Storage Keys](docs/RESERVED_STORAGE_KEYS.md) for the list of keys not yet available for reuse, and [testutils/tests/README.md](testutils/tests/README.md) for information about the automated validation tests.

**Run validation tests:**

```bash
# Catalogue check + live source scan
cargo test --package testutils storage_key -- --nocapture
```

## Common Patterns

### Storage scope

- Most contracts use `env.storage().instance()`.
- `savings_goals` additionally writes `NEXT_ID` and `GOALS` to `persistent()` in `init` (legacy bootstrap path), while runtime operations use instance keys.

### TTL bump strategy

- Most contracts define:
  - `INSTANCE_LIFETIME_THRESHOLD = 17280` (~1 day)
  - `INSTANCE_BUMP_AMOUNT = 518400` (~30 days)
- Archive-enabled contracts also define:
  - `ARCHIVE_LIFETIME_THRESHOLD = 17280`
  - `ARCHIVE_BUMP_AMOUNT = 2592000` (~180 days)
- Important implementation detail:
  - Archive bump helpers still call `instance().extend_ttl(...)`; they extend the contract instance entry TTL, not a separate archive namespace.

### Shared TTL-bump helpers (remitwise-common)

`remitwise-common` exports three helpers that centralise the canonical TTL policy:

| Helper                      | Threshold                                 | Bump                               | Purpose                    |
| --------------------------- | ----------------------------------------- | ---------------------------------- | -------------------------- |
| `bump_instance(env)`        | `INSTANCE_LIFETIME_THRESHOLD` (1 day)     | `INSTANCE_BUMP_AMOUNT` (30 days)   | Active instance data       |
| `bump_persistent(env, key)` | `PERSISTENT_LIFETIME_THRESHOLD` (15 days) | `PERSISTENT_BUMP_AMOUNT` (60 days) | Persistent storage entries |
| `bump_archive(env)`         | `ARCHIVE_LIFETIME_THRESHOLD` (1 day)      | `ARCHIVE_BUMP_AMOUNT` (150 days)   | Archive instance window    |

Using these helpers prevents the common mistake of swapping `threshold` and `bump` arguments. See [docs/ttl-bump-helpers.md](docs/ttl-bump-helpers.md) for usage guidance.

### ID allocation patterns

- Monotonic counters via `NEXT_*` keys:
  - `NEXT_ID`, `NEXT_SSCH`, `NEXT_PSCH`, `NEXT_RSCH`, `NEXT_TX`
- Pattern is generally:
  - read current counter with default `0` (or `1` for `family_wallet` init),
  - increment by `+1`,
  - persist updated counter before/after writing object map.

## remittance_split

### Keys and value types (instance storage)

| Key         | Type                           | Notes                                                        |
| ----------- | ------------------------------ | ------------------------------------------------------------ |
| `CONFIG`    | `SplitConfig`                  | Owner + percentages + initialized flag                       |
| `NONCES`    | `Map<Address, u64>`            | Replay protection for owner-authorized mutating calls        |
| `AUDIT`     | `Vec<AuditEntry>`              | Rotating audit log, max `MAX_AUDIT_ENTRIES` (100)            |
| `REM_SCH`   | `Map<u32, RemittanceSchedule>` | Remittance schedules                                         |
| `NEXT_RSCH` | `u32`                          | Next remittance schedule ID                                  |
| `PAUSE_ADM` | `Address`                      | Pause admin                                                  |
| `PAUSED`    | `bool`                         | Global pause flag                                            |
| `PAUSED_AT` | `u64`                          | Timestamp when contract was paused                           |
| `UPG_ADM`   | `Address`                      | Upgrade admin                                                |
| `VERSION`   | `u32`                          | Contract version                                             |

### Keys and value types (persistent storage)

| Key               | Type                 | Notes                                                     |
| ----------------- | -------------------- | --------------------------------------------------------- |
| `Schedule(u32)`   | `RemittanceSchedule` | Individual remittance schedule                            |
| `OwnerSchedules`  | `Vec<u32>`           | Per-owner index of schedule IDs, ordered by ID ascending  |

### TTL and IDs

- **Instance TTL**: `CONFIG`, `SPLIT`, and other administrative keys are bumped on every access (read/write) using `INSTANCE_BUMP_AMOUNT` / `INSTANCE_LIFETIME_THRESHOLD`.
- **Persistent TTL**: `RemittanceSchedule` and the `OwnerSchedules` index are bumped on every access using `PERSISTENT_BUMP_AMOUNT` / `PERSISTENT_LIFETIME_THRESHOLD`.
- Schedule IDs allocate from `NEXT_RSCH` (`0 -> 1 -> 2 ...`).

### Savings Goals Storage (Scalable DataKey Pattern)

### Keys and value types (instance storage)

| Key         | Type                        | Notes                                  |
| ----------- | --------------------------- | -------------------------------------- |
| `GOALS`     | `Map<u32, SavingsGoal>`     | Primary goal records                   |
| `NEXT_ID`   | `u32`                       | Next savings goal ID                   |
| `SAV_SCH`   | `Map<u32, SavingsSchedule>` | Recurring savings schedules            |
| `NEXT_SSCH` | `u32`                       | Next savings schedule ID               |
| `NONCES`    | `Map<Address, u64>`         | Snapshot import nonce tracking         |
| `AUDIT`     | `Vec<AuditEntry>`           | Rotating audit log, max 100            |
| `PAUSE_ADM` | `Address`                   | Pause admin                            |
| `PAUSED`    | `bool`                      | Global pause flag                      |
| `PausedSince` | `u64`                    | Timestamp when contract was paused     |
| `PAUSED_FN` | `Map<Symbol, bool>`         | Per-function pause switches            |
| `UNP_AT`    | `u64`                       | Optional time-locked unpause timestamp |
| `UPG_ADM`   | `Address`                   | Upgrade admin                          |
| `VERSION`   | `u32`                       | Contract version                       |

| Key       | Type                    | Notes                           |
| --------- | ----------------------- | ------------------------------- |
| `NEXT_ID` | `u32`                   | Initialized in `init` if absent |
| `GOALS`   | `Map<u32, SavingsGoal>` | Initialized in `init` if absent |

### TTL and IDs

- Instance TTL bumps on state-changing operations.
- Goal IDs: `NEXT_ID`.
- Schedule IDs: `NEXT_SSCH`.
- Migration note: both persistent and instance use `NEXT_ID`/`GOALS`; runtime logic reads instance keys.

## bill_payments

### Keys and value types (instance storage)

| Key         | Type                     | Notes                                                                                                                        |
| ----------- | ------------------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| `BILLS`     | `Map<u32, Bill>`         | Active bill records                                                                                                          |
| `NEXT_ID`   | `u32`                    | Next bill ID                                                                                                                 |
| `ARCH_BILL` | `Map<u32, ArchivedBill>` | Archived paid bills                                                                                                          |
| `ARCH_IDX`  | `Map<Address, Vec<u32>>` | Per-owner index of archived bill IDs (ascending order); used by `get_archived_bills_page` for O(limit) gas-bounded retrieval |
| `STOR_STAT` | `StorageStats`           | Aggregated storage metrics                                                                                                   |
| `PAUSE_ADM` | `Address`                | Pause admin                                                                                                                  |
| `PAUSED`    | `bool`                   | Global pause flag                                                                                                            |
| `PAUSED_AT` | `u64`                    | Timestamp when contract was paused                                                                                           |
| `PAUSED_FN` | `Map<Symbol, bool>`      | Per-function pause switches                                                                                                  |
| `UNP_AT`    | `u64`                    | Optional unpause timestamp                                                                                                   |
| `UPG_ADM`   | `Address`                | Upgrade admin                                                                                                                |
| `VERSION`   | `u32`                    | Contract version                                                                                                             |

### TTL and IDs

- Uses both `extend_instance_ttl` and `extend_archive_ttl` (instance-scope TTL extension).
- Bill IDs allocate from `NEXT_ID`.
- Recurring bill creation in `pay_bill` and `batch_pay_bills` also consumes `NEXT_ID`.

## insurance

### Keys and value types (instance storage)

| Key         | Type                        | Notes                       |
| ----------- | --------------------------- | --------------------------- |
| `POLICIES`  | `Map<u32, InsurancePolicy>` | Insurance policy records    |
| `NEXT_ID`   | `u32`                       | Next policy ID              |
| `PREM_SCH`  | `Map<u32, PremiumSchedule>` | Premium schedules           |
| `NEXT_PSCH` | `u32`                       | Next premium schedule ID    |
| `PAUSE_ADM` | `Address`                   | Pause admin                 |
| `PAUSED`    | `bool`                      | Global pause flag           |
| `PAUSED_FN` | `Map<Symbol, bool>`         | Per-function pause switches |
| `UNP_AT`    | `u64`                       | Optional unpause timestamp  |
| `UPG_ADM`   | `Address`                   | Upgrade admin               |
| `VERSION`   | `u32`                       | Contract version            |

### TTL and IDs

- Instance TTL bumps on mutating policy/schedule operations.
- Policy IDs allocate from `NEXT_ID`.
- Premium schedule IDs allocate from `NEXT_PSCH`.

## family_wallet

### Keys and value types (instance storage)

| Key         | Type                            | Notes                                          |
| ----------- | ------------------------------- | ---------------------------------------------- |
| `OWNER`     | `Address`                       | Wallet owner                                   |
| `MEMBERS`   | `Map<Address, FamilyMember>`    | Family members and roles                       |
| `MS_WDRAW`  | `MultiSigConfig`                | Multisig config for large withdrawals          |
| `MS_SPLIT`  | `MultiSigConfig`                | Multisig config for split changes              |
| `MS_ROLE`   | `MultiSigConfig`                | Multisig config for role changes               |
| `MS_EMERG`  | `MultiSigConfig`                | Multisig config for emergency transfer type    |
| `MS_POL`    | `MultiSigConfig`                | Multisig config for policy cancellation        |
| `MS_REG`    | `MultiSigConfig`                | Config key for regular withdrawals (read path) |
| `PEND_TXS`  | `Map<u64, PendingTransaction>`  | Pending multisig transactions                  |
| `EXEC_TXS`  | `Map<u64, bool>`                | Executed transaction markers                   |
| `NEXT_TX`   | `u64`                           | Next pending tx ID                             |
| `EM_CONF`   | `EmergencyConfig`               | Emergency transfer constraints                 |
| `EM_MODE`   | `bool`                          | Emergency mode toggle                          |
| `EM_LAST`   | `u64`                           | Last emergency transfer timestamp              |
| `ARCH_TX`   | `Map<u64, ArchivedTransaction>` | Archived executed transaction metadata         |
| `STOR_STAT` | `StorageStats`                  | Storage usage stats                            |
| `ROLE_EXP`  | `Map<Address, u64>`             | Role expiry timestamps                         |
| `PAUSED`    | `bool`                          | Global pause flag                              |
| `PAUSE_ADM` | `Address`                       | Pause admin                                    |
| `UPG_ADM`   | `Address`                       | Upgrade admin                                  |
| `VERSION`   | `u32`                           | Contract version                               |
| `ACC_AUDIT` | `Vec<AccessAuditEntry>`         | Rolling access audit trail, capped at 100      |

### TTL and IDs

- Uses instance and archive TTL helpers (both extend instance TTL).
- `NEXT_TX` initialized to `1`.
- Immediate transactions return `tx_id = 0` and do not consume `NEXT_TX`.
- Only multisig-required proposals consume `NEXT_TX`.

## reporting

### Keys and value types (instance storage)

| Key         | Type                                         | Notes                                        |
| ----------- | -------------------------------------------- | -------------------------------------------- |
| `ADMIN`     | `Address`                                    | Reporting admin                              |
| `ADDRS`     | `ContractAddresses`                          | Cross-contract address registry              |
| `REPORTS`   | `Map<(Address, u64), FinancialHealthReport>` | Active reports keyed by `(user, period_key)` |
| `ARCH_RPT`  | `Map<(Address, u64), ArchivedReport>`        | Archived report summaries                    |
| `STOR_STAT` | `StorageStats`                               | Active/archive counts                        |

### TTL and IDs

- Uses instance and archive TTL helpers (instance-scope extension).
- No `NEXT_*` counter.
- Key identity pattern is composite tuple key `(Address, period_key)`; `period_key` is caller-defined.

## orchestrator

### Keys and value types (instance storage)

| Key     | Type                          | Notes                        |
| ------- | ----------------------------- | ---------------------------- |
| `STATS` | `ExecutionStats`              | Aggregate execution counters |
| `AUDIT` | `Vec<OrchestratorAuditEntry>` | Rotating audit log, max 100  |

### TTL and IDs

- Defines instance TTL constants and helper `extend_instance_ttl`.
- No numeric ID counters.
- Current implementation note: storage write helpers (`update_execution_stats`, `append_audit_entry`) exist but are not wired into public execution paths.

## data_migration

### Storage usage

- No Soroban storage keys (not an on-chain contract state machine).
- This crate provides off-chain serialization/validation utilities:
  - schema/version checks
  - checksum validation
  - export/import formats (JSON, binary, CSV, encrypted payload wrappers)

### TTL and IDs

- No on-chain TTL handling.
- No on-chain ID allocation counters.

## emergency_killswitch

### Keys and value types (instance storage)

| Key             | Type                    | Notes                                                           |
|-----------------|-------------------------|-----------------------------------------------------------------|
| `ADMIN`         | `Address`               | Killswitch admin                                                |
| `KILL_SW`       | `u64`                   | Kill-switch epoch (replay protection for `transfer_admin`)       |
| `SIGNERS`       | `Vec<Address>`          | Signer set for threshold-based activation                       |
| `SIGNR_THR`     | `u32`                   | Signer threshold (required approvals)                           |
| `SIGNR_EP`      | `u64`                   | Signer epoch (bumped on set rotation)                           |
| `ACTVT_EP`      | `u64`                   | Activation epoch (present while threshold activation is active) |
| `ACTVT_SC`      | `PauseScope`            | Active scope during threshold activation                        |
| `RCVRY_RDY`     | `u64`                   | Timestamp after which recovery is allowed                       |
| `SCPE_WPS`      | `bool`                  | Whether the scope was already paused before activation           |
| `GLOBL_P`       | `bool`                  | Global pause flag                                               |
| `PAUSED_AT`     | `u64`                   | Timestamp when globally paused                                  |
| `P_RSN`         | `Symbol`                | Optional reason for the pause                                   |
| `UNP_AT`        | `u64`                   | Scheduled unpause timestamp                                     |
| `MOD_P(symbol)` | `bool`                  | Per-module pause flag                                           |
| `FN_P(symbol)`  | `Vec<Symbol>`           | Paused functions per module (bounded at 10)                     |
| `STOR_VER`      | `u32`                   | Storage schema version (0 = pre-versioning)                     |
| `SNAP`          | `EmergencyStateSnapshot`| Pre-upgrade snapshot of full emergency state                    |
| `SNAP_TS`       | `u64`                   | Timestamp when snapshot was taken                               |
| `MIGRPRG`       | `MigrationProgress`     | Resumable migration step tracker                                |

### Storage version & migration

- `storage_version()` returns the on-chain version (0 for legacy deployments).
- `migrate_storage()` advances one version step per call, recording progress in `MIGRPRG`.
- `pre_upgrade()` / `restore_from_snapshot()` / `discard_snapshot()` manage upgrade snapshots.
- Snapshots expire after 24 hours (`SNAPSHOT_TTL`).

### TTL and IDs

- Uses instance storage for all keys.
- No persistent storage or TTL bump helpers.
- No ID counters — epoch-based tracking only.

## Audit and Migration Implications

- Audit trails are explicit in:
  - `remittance_split` (`AUDIT`)
  - `savings_goals` (`AUDIT`)
  - `family_wallet` (`ACC_AUDIT`)
  - `orchestrator` (`AUDIT`, presently helper-gated)
- Migration-oriented snapshot/export paths are explicit in:
  - `remittance_split` (`export_snapshot` / `import_snapshot`)
  - `savings_goals` (`GoalsExportSnapshot`, nonce-protected import/export)
  - `data_migration` crate (off-chain format conversion and integrity checks)
