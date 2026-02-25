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

### ID allocation patterns

- Monotonic counters via `NEXT_*` keys:
  - `NEXT_ID`, `NEXT_SSCH`, `NEXT_PSCH`, `NEXT_RSCH`, `NEXT_TX`
- Pattern is generally:
  - read current counter with default `0` (or `1` for `family_wallet` init),
  - increment by `+1`,
  - persist updated counter before/after writing object map.

## remittance_split

### Keys and value types (instance storage)

| Key | Type | Notes |
|---|---|---|
| `CONFIG` | `SplitConfig` | Owner + percentages + initialized flag |
| `SPLIT` | `Vec<u32>` | Ordered percentages: `[spending, savings, bills, insurance]` |
| `NONCES` | `Map<Address, u64>` | Replay protection for owner-authorized mutating calls |
| `AUDIT` | `Vec<AuditEntry>` | Rotating audit log, max `MAX_AUDIT_ENTRIES` (100) |
| `REM_SCH` | `Map<u32, RemittanceSchedule>` | Remittance schedules |
| `NEXT_RSCH` | `u32` | Next remittance schedule ID |
| `PAUSE_ADM` | `Address` | Pause admin |
| `PAUSED` | `bool` | Global pause flag |
| `UPG_ADM` | `Address` | Upgrade admin |
| `VERSION` | `u32` | Contract version |

### TTL and IDs

- TTL bumps on mutating flows via `extend_instance_ttl`.
- Schedule IDs allocate from `NEXT_RSCH` (`0 -> 1 -> 2 ...`).

## savings_goals

### Keys and value types (instance storage)

| Key | Type | Notes |
|---|---|---|
| `GOALS` | `Map<u32, SavingsGoal>` | Primary goal records |
| `NEXT_ID` | `u32` | Next savings goal ID |
| `SAV_SCH` | `Map<u32, SavingsSchedule>` | Recurring savings schedules |
| `NEXT_SSCH` | `u32` | Next savings schedule ID |
| `NONCES` | `Map<Address, u64>` | Snapshot import nonce tracking |
| `AUDIT` | `Vec<AuditEntry>` | Rotating audit log, max 100 |
| `PAUSE_ADM` | `Address` | Pause admin |
| `PAUSED` | `bool` | Global pause flag |
| `PAUSED_FN` | `Map<Symbol, bool>` | Per-function pause switches |
| `UNP_AT` | `u64` | Optional time-locked unpause timestamp |
| `UPG_ADM` | `Address` | Upgrade admin |
| `VERSION` | `u32` | Contract version |

### Keys and value types (persistent storage)

| Key | Type | Notes |
|---|---|---|
| `NEXT_ID` | `u32` | Initialized in `init` if absent |
| `GOALS` | `Map<u32, SavingsGoal>` | Initialized in `init` if absent |

### TTL and IDs

- Instance TTL bumps on state-changing operations.
- Goal IDs: `NEXT_ID`.
- Schedule IDs: `NEXT_SSCH`.
- Migration note: both persistent and instance use `NEXT_ID`/`GOALS`; runtime logic reads instance keys.

## bill_payments

### Keys and value types (instance storage)

| Key | Type | Notes |
|---|---|---|
| `BILLS` | `Map<u32, Bill>` | Active bill records |
| `NEXT_ID` | `u32` | Next bill ID |
| `ARCH_BILL` | `Map<u32, ArchivedBill>` | Archived paid bills |
| `STOR_STAT` | `StorageStats` | Aggregated storage metrics |
| `PAUSE_ADM` | `Address` | Pause admin |
| `PAUSED` | `bool` | Global pause flag |
| `PAUSED_FN` | `Map<Symbol, bool>` | Per-function pause switches |
| `UNP_AT` | `u64` | Optional unpause timestamp |
| `UPG_ADM` | `Address` | Upgrade admin |
| `VERSION` | `u32` | Contract version |

### TTL and IDs

- Uses both `extend_instance_ttl` and `extend_archive_ttl` (instance-scope TTL extension).
- Bill IDs allocate from `NEXT_ID`.
- Recurring bill creation in `pay_bill` and `batch_pay_bills` also consumes `NEXT_ID`.

## insurance

### Keys and value types (instance storage)

| Key | Type | Notes |
|---|---|---|
| `POLICIES` | `Map<u32, InsurancePolicy>` | Insurance policy records |
| `NEXT_ID` | `u32` | Next policy ID |
| `PREM_SCH` | `Map<u32, PremiumSchedule>` | Premium schedules |
| `NEXT_PSCH` | `u32` | Next premium schedule ID |
| `PAUSE_ADM` | `Address` | Pause admin |
| `PAUSED` | `bool` | Global pause flag |
| `PAUSED_FN` | `Map<Symbol, bool>` | Per-function pause switches |
| `UNP_AT` | `u64` | Optional unpause timestamp |
| `UPG_ADM` | `Address` | Upgrade admin |
| `VERSION` | `u32` | Contract version |

### TTL and IDs

- Instance TTL bumps on mutating policy/schedule operations.
- Policy IDs allocate from `NEXT_ID`.
- Premium schedule IDs allocate from `NEXT_PSCH`.

## family_wallet

### Keys and value types (instance storage)

| Key | Type | Notes |
|---|---|---|
| `OWNER` | `Address` | Wallet owner |
| `MEMBERS` | `Map<Address, FamilyMember>` | Family members and roles |
| `MS_WDRAW` | `MultiSigConfig` | Multisig config for large withdrawals |
| `MS_SPLIT` | `MultiSigConfig` | Multisig config for split changes |
| `MS_ROLE` | `MultiSigConfig` | Multisig config for role changes |
| `MS_EMERG` | `MultiSigConfig` | Multisig config for emergency transfer type |
| `MS_POL` | `MultiSigConfig` | Multisig config for policy cancellation |
| `MS_REG` | `MultiSigConfig` | Config key for regular withdrawals (read path) |
| `PEND_TXS` | `Map<u64, PendingTransaction>` | Pending multisig transactions |
| `EXEC_TXS` | `Map<u64, bool>` | Executed transaction markers |
| `NEXT_TX` | `u64` | Next pending tx ID |
| `EM_CONF` | `EmergencyConfig` | Emergency transfer constraints |
| `EM_MODE` | `bool` | Emergency mode toggle |
| `EM_LAST` | `u64` | Last emergency transfer timestamp |
| `ARCH_TX` | `Map<u64, ArchivedTransaction>` | Archived executed transaction metadata |
| `STOR_STAT` | `StorageStats` | Storage usage stats |
| `ROLE_EXP` | `Map<Address, u64>` | Role expiry timestamps |
| `PAUSED` | `bool` | Global pause flag |
| `PAUSE_ADM` | `Address` | Pause admin |
| `UPG_ADM` | `Address` | Upgrade admin |
| `VERSION` | `u32` | Contract version |
| `ACC_AUDIT` | `Vec<AccessAuditEntry>` | Rolling access audit trail, capped at 100 |

### TTL and IDs

- Uses instance and archive TTL helpers (both extend instance TTL).
- `NEXT_TX` initialized to `1`.
- Immediate transactions return `tx_id = 0` and do not consume `NEXT_TX`.
- Only multisig-required proposals consume `NEXT_TX`.

## reporting

### Keys and value types (instance storage)

| Key | Type | Notes |
|---|---|---|
| `ADMIN` | `Address` | Reporting admin |
| `ADDRS` | `ContractAddresses` | Cross-contract address registry |
| `REPORTS` | `Map<(Address, u64), FinancialHealthReport>` | Active reports keyed by `(user, period_key)` |
| `ARCH_RPT` | `Map<(Address, u64), ArchivedReport>` | Archived report summaries |
| `STOR_STAT` | `StorageStats` | Active/archive counts |

### TTL and IDs

- Uses instance and archive TTL helpers (instance-scope extension).
- No `NEXT_*` counter.
- Key identity pattern is composite tuple key `(Address, period_key)`; `period_key` is caller-defined.

## orchestrator

### Keys and value types (instance storage)

| Key | Type | Notes |
|---|---|---|
| `STATS` | `ExecutionStats` | Aggregate execution counters |
| `AUDIT` | `Vec<OrchestratorAuditEntry>` | Rotating audit log, max 100 |

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
