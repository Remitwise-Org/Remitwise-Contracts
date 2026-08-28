# Governance Parameters

**Target Audience:** Contributors

Every configurable parameter across the Remitwise contracts — hardcoded constants (changeable only via contract upgrade) and storage-backed parameters (changeable at runtime by authorized roles) — with their types, bounds, defaults, and governance mechanism.

---

## Table of Contents

1. [remitwise-common (Shared Constants)](#1-remitwise-common-shared-constants)
2. [family_wallet](#2-family_wallet)
3. [bill_payments](#3-bill_payments)
4. [remittance_split](#4-remittance_split)
5. [insurance](#5-insurance)
6. [savings_goals](#6-savings_goals)
7. [orchestrator](#7-orchestrator)
8. [reporting](#8-reporting)
9. [emergency_killswitch](#9-emergency_killswitch)
10. [data_migration](#10-data_migration)
11. [Cross-Cutting Admin Roles](#11-cross-cutting-admin-roles)

---

## 1. remitwise-common (Shared Constants)

Defined in `remitwise-common/src/lib.rs`. All are hardcoded — changeable only by upgrading the crate.

### Pagination

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `DEFAULT_PAGE_LIMIT` | `u32` | 1 | 50 | **20** | Default page size for paginated queries |
| `MAX_PAGE_LIMIT` | `u32` | 20 | — | **50** | Hard cap on pagination page size |

### Storage TTL (ledger counts; ~5 s/ledger)

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `INSTANCE_LIFETIME_THRESHOLD` | `u32` | — | — | **120_960 (7 days)** | TTL threshold for instance storage |
| `INSTANCE_BUMP_AMOUNT` | `u32` | — | — | **518_400 (30 days)** | TTL bump amount for instance storage |
| `PERSISTENT_LIFETIME_THRESHOLD` | `u32` | — | — | **259_200 (15 days)** | TTL threshold for persistent storage |
| `PERSISTENT_BUMP_AMOUNT` | `u32` | — | — | **1_036_800 (60 days)** | TTL bump amount for persistent storage |
| `ARCHIVE_LIFETIME_THRESHOLD` | `u32` | — | — | **120_960 (7 days)** | TTL threshold for archived storage |
| `ARCHIVE_BUMP_AMOUNT` | `u32` | — | — | **3_110_400 (180 days)** | TTL bump amount for archived storage |

### Security & Rate Limits

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `SIGNATURE_EXPIRATION` | `u64` | — | — | **86_400 (24 h)** | How long transaction signatures remain valid (seconds) |
| `MAX_BATCH_SIZE` | `u32` | — | — | **50** | Max items in batch operations |
| `MAX_BYTES_RETURN` | `u32` | — | — | **8_192** | Max bytes returned from public entry points (DoS guard) |
| `MIN_TRANSFER` | `i128` | — | — | **100** (stroops) | Anti-dust minimum transfer amount |
| `RATE_LIMIT_WINDOW_SECONDS` | `u64` | — | — | **86_400 (24 h)** | Rate-limit time window |

### Rate / Percent Arithmetic

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `BASIS_POINTS` | `u32` | — | — | **10_000** | Basis-points denominator (100% = 10 000 bps) |
| `PRO_RATA_MAX_TOTAL_WEIGHT` | `u32` | — | — | **10_000** | Max total weight for pro-rata distribution |

### Snapshots & Migration

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `SNAPSHOT_VERSION` | `u32` | — | — | **1** | Pre-upgrade snapshot schema version |
| `SNAPSHOT_MAX_AGE_SECS` | `u64` | — | — | **2_592_000 (30 days)** | Max age of snapshot before restore is rejected |

### Token Registry (`remitwise-common/src/tokens.rs`)

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `MAX_CURRENCY_LEN` | `u32` | — | — | **10** | Max length of currency code strings |
| `DEFAULT_CURRENCY` | `&str` | — | — | `"XLM"` | Default currency code |

---

## 2. family_wallet

Defined in `family_wallet/src/lib.rs`.

### Hardcoded Constants (changeable only via upgrade)

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `MIN_THRESHOLD` | `u32` | — | — | **1** | Minimum multisig signing threshold |
| `MAX_SIGNERS` | `u32` | — | — | **20** | Max multisig signers per config |
| `MAX_THRESHOLD` | `u32` | — | — | **100** | Max multisig threshold |
| `MAX_BATCH_MEMBERS` | `u32` | — | — | **30** | Max members per batch add/remove |
| `MAX_FAMILY_MEMBERS` | `u32` | — | — | **30** | Max family members overall |
| `MAX_ACCESS_AUDIT_ENTRIES` | `u32` | — | — | **200** | Max audit entries retained |
| `MAX_AUDIT_PAGE_LIMIT` | `u32` | — | — | **50** | Max page size for audit queries |
| `MAX_PROPOSAL_EXPIRY` | `u64` | — | — | **604_800 (7 days)** | Max multisig proposal expiry window |
| `DEFAULT_PROPOSAL_EXPIRY` | `u64` | 0 | 604_800 | **86_400 (24 h)** | Default proposal expiry when unset |
| `DEFAULT_MULTISIG_SPENDING_LIMIT` | `i128` | — | — | **1_000 XLM** (stroops) | Default spending limit for multisig configs |
| `DEFAULT_EMERGENCY_MAX_AMOUNT` | `i128` | — | — | **10_000 XLM** (stroops) | Default emergency single-transfer cap |
| `DEFAULT_EMERGENCY_DAILY_LIMIT` | `i128` | — | — | **100_000 XLM** (stroops) | Default emergency daily limit |
| `MAX_ARCHIVE_ENTRIES` | `u32` | — | — | **500** | Max archived transactions retained |
| `MAX_ARCHIVE_PAGE_LIMIT` | `u32` | — | — | **100** | Max page size for archived tx queries |

### Storage-Backed Parameters (set at runtime)

| Parameter | Type | Min | Max | Default | Set By | Description |
| --- | --- | --- | --- | --- | --- | --- |
| `MultiSigConfig.threshold` | `u32` | 1 | 100 | — | `configure_multisig` (owner/admin) | Signatures required per tx type |
| `MultiSigConfig.signers` | `Vec<Address>` | 1 addr | 20 addrs | — | `configure_multisig` (owner/admin) | Authorized signer list |
| `MultiSigConfig.spending_limit` | `i128` | 0 | — | — | `configure_multisig` (owner/admin) | Spending cap for the config |
| `EmergencyConfig.max_amount` | `i128` | > 0 | — | 10_000 XLM | `configure_emergency` (owner/admin) | Max single emergency transfer |
| `EmergencyConfig.cooldown` | `u64` | 0 | — | — | `configure_emergency` (owner/admin) | Cooldown between emergency transfers |
| `EmergencyConfig.min_balance` | `i128` | ≥ 0 | — | — | `configure_emergency` (owner/admin) | Min post-transfer balance |
| `EmergencyConfig.daily_limit` | `i128` | ≥ 0 | — | 100_000 XLM | `configure_emergency` (owner/admin) | Max sum of emergency transfers per day |
| `ProposalExpiry` | `u64` | 0 | 604_800 | 86_400 | `set_proposal_expiry` (owner) | Proposal expiration window (seconds) |
| Spending limit per member | `i128` | ≥ 0 | — | — | `update_spending_limit` (owner/admin) | Per-member daily spending cap |
| `PrecisionSpendingLimit.limit` | `i128` | ≥ 0 | — | — | `set_precision_spending_limit` (owner/admin) | Cumulative spending limit for precision controls |
| `PrecisionSpendingLimit.min_precision` | `i128` | > 0 | — | — | `set_precision_spending_limit` (owner/admin) | Min unit for precision rounding |
| `PrecisionSpendingLimit.max_single_tx` | `i128` | > 0 | ≤ limit | — | `set_precision_spending_limit` (owner/admin) | Max per-transaction under precision limit |
| `PrecisionSpendingLimit.enable_rollover` | `bool` | — | — | — | `set_precision_spending_limit` (owner/admin) | Whether unused daily limit rolls over |
| Role expiry per member | `Option<u64>` | — | — | `None` | `set_role_expiry` (admin) | Role expiry timestamp per member |

---

## 3. bill_payments

Defined in `bill_payments/src/lib.rs`.

### Hardcoded Constants (changeable only via upgrade)

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `MAX_BILLS_PER_OWNER` | `u32` | — | — | **1_000** | Max active bills per owner |
| `MAX_FREQUENCY_DAYS` | `u32` | — | — | **36_500 (100 years)** | Max bill recurrence frequency (days) |
| `MAX_NAME_LEN` | `u32` | — | — | **64** | Max bill name length (bytes) |
| `MAX_EXTERNAL_REF_LEN` | `u32` | — | — | **64** | Max external ref string length |
| `MIN_EXTERNAL_REF_LEN` | `u32` | — | — | **1** | Min external ref string length |
| `MIN_SCHEDULE_INTERVAL` | `u64` | — | — | **3_600 (1 h)** | Min recurring bill schedule interval (seconds) |
| `MAX_SCHEDULE_LEAD_TIME` | `u64` | — | — | **31_536_000 (1 year)** | Max lead time for scheduled bills (seconds) |
| `MAX_BILL_SCHEDULES_PER_OWNER` | `u32` | — | — | **50** | Max bill schedules per owner |
| `ADMIN_GRANT_TTL` | `u64` | — | — | **2_592_000 (30 days)** | Admin grant time-to-live before refresh needed (seconds) |
| `CREATE_BILL_RATE_LIMIT` | `u32` | — | — | **100** | Max bill creates per address per 24 h window |
| `PAY_BILL_RATE_LIMIT` | `u32` | — | — | **200** | Max bill pays per address per 24 h window |
| `CANCEL_BILL_RATE_LIMIT` | `u32` | — | — | **50** | Max bill cancels per address per 24 h window |

### Storage-Backed Parameters (set at runtime)

| Parameter | Type | Min | Max | Default | Set By | Description |
| --- | --- | --- | --- | --- | --- | --- |
| PauseAdmin | `Address` | — | — | None (bootstrap) | `set_pause_admin` (owner) | Who can pause/unpause bills |
| UpgradeAdmin | `Address` | — | — | None (bootstrap) | `set_upgrade_admin` (owner) | Who can upgrade bill contract |
| `Paused` (global) | `bool` | — | — | `false` | `pause` / `unpause` (pause admin) | Global pause state |
| `PausedFunctions` | `Map<Symbol,bool>` | — | — | empty | `pause` / `unpause` (pause admin) | Function-level pause flags |
| `UnpauseAt` | `Option<u64>` | — | — | `None` | `schedule_unpause` | Timelocked unpause timestamp |

---

## 4. remittance_split

Defined in `remittance_split/src/lib.rs`.

### Hardcoded Constants (changeable only via upgrade)

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `MAX_USED_NONCES_PER_ADDR` | `u32` | — | — | **256** | Max tracked nonces per address before pruning |
| `MAX_SCHEDULES_PER_OWNER` | `u32` | — | — | **50** | Max remittance schedules per owner |
| `MIN_SCHEDULE_INTERVAL` | `u64` | — | — | **3_600 (1 h)** | Min recurrence interval for schedules (seconds) |
| `MAX_SCHEDULE_LEAD_TIME` | `u64` | — | — | **31_536_000 (1 year)** | Max lead time for schedules (seconds) |
| `MAX_DEADLINE_WINDOW_SECS` | `u64` | — | — | **3_600 (1 h)** | Max window for transaction deadlines (seconds) |

### Storage-Backed Parameters (set at runtime)

| Parameter | Type | Min | Max | Default | Set By | Description |
| --- | --- | --- | --- | --- | --- | --- |
| `SplitConfig.spending_percent` | `u32` (bps) | 0 | 10_000 | — | `update_split` or `create_split` (owner) | % of funds allocated to spending |
| `SplitConfig.savings_percent` | `u32` (bps) | 0 | 10_000 | — | `update_split` or `create_split` (owner) | % of funds allocated to savings |
| `SplitConfig.bills_percent` | `u32` (bps) | 0 | 10_000 | — | `update_split` or `create_split` (owner) | % of funds allocated to bills |
| `SplitConfig.insurance_percent` | `u32` (bps) | 0 | 10_000 | — | `update_split` or `create_split` (owner) | % of funds allocated to insurance |
| SplitConfig (sum of 4 percents) | — | — | Must = 10_000 | — | validated in `validate_percentages` | All percentages must sum to 100% |
| PauseAdmin | `Address` | — | — | Owner | `set_pause_admin` | Who can pause/unpause splits |
| UpgradeAdmin | `Address` | — | — | None (bootstrap) | `set_upgrade_admin` | Who can upgrade split contract |

---

## 5. insurance

Defined in `insurance/src/lib.rs`.

### Hardcoded Constants (changeable only via upgrade)

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `MAX_POLICIES` | `u32` | — | — | **1_000** | Max policies per contract |
| `THIRTY_DAYS_SECS` | `u64` | — | — | **2_592_000** | Default premium payment interval (seconds) |
| `MAX_TENURE_SECS` | `u64` | — | — | **86_400 (24 h)** | Min time before deactivated policy can be reactivated |
| `MAX_NAME_LEN` | `u32` | — | — | **64** | Max policy name length (bytes) |
| `MAX_EXT_REF_LEN` | `u32` | — | — | **128** | Max external ref string length |
| `MIN_SCHEDULE_INTERVAL` | `u64` | — | — | **3_600 (1 h)** | Min premium schedule recurrence interval (seconds) |
| `MAX_SCHEDULE_LEAD_TIME` | `u64` | — | — | **31_536_000 (1 year)** | Max lead time for premium schedules (seconds) |
| `MAX_SCHEDULES_PER_OWNER` | `u32` | — | — | **50** | Max premium schedules per owner |

### Per-Type Premium & Coverage Bounds (hardcoded in `TypeConstraints`)

All values in stroops.

| CoverageType | Parameter | Min | Max |
| --- | --- | --- | --- |
| **Health** | `min_premium` | 1 | 500_000_000_000 |
| | `max_premium` | — | — |
| | `min_coverage` | 1 | 100_000_000_000_000 |
| | `max_coverage` | — | — |
| **Life** | `min_premium` | 1 | 1_000_000_000_000 |
| | `max_premium` | — | — |
| | `min_coverage` | 1 | 500_000_000_000_000 |
| | `max_coverage` | — | — |
| **Property** | `min_premium` | 1 | 2_000_000_000_000 |
| | `max_premium` | — | — |
| | `min_coverage` | 1 | 1_000_000_000_000_000 |
| | `max_coverage` | — | — |
| **Auto** | `min_premium` | 1 | 750_000_000_000 |
| | `max_premium` | — | — |
| | `min_coverage` | 1 | 200_000_000_000_000 |
| | `max_coverage` | — | — |
| **Liability** | `min_premium` | 1 | 400_000_000_000 |
| | `max_premium` | — | — |
| | `min_coverage` | 1 | 50_000_000_000_000 |
| | `max_coverage` | — | — |

### Storage-Backed Parameters (set at runtime)

Unlike other contracts, Insurance has no separate PauseAdmin/UpgradeAdmin roles — the contract owner controls everything.

| Parameter | Type | Min | Max | Default | Set By | Description |
| --- | --- | --- | --- | --- | --- | --- |
| ContractOwner | `Address` | — | — | Set in `init` | `initialize` (deployer) | Who can create policies, set external refs, upgrade |

---

## 6. savings_goals

Defined in `savings_goals/src/lib.rs`.

### Hardcoded Constants (changeable only via upgrade)

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `MAX_SAFE_GOAL_BALANCE` | `i128` | — | — | **i128::MAX / 2** | Max goal balance (overflow guard) |
| `MAX_GOAL_NAME_LEN_BYTES` | `u32` | — | — | **32** | Max goal name length (bytes) |
| `MAX_GOALS_PER_OWNER` | `u32` | — | — | **2_000** | Max goals (active + archived) per owner |
| `DEFAULT_PAGE_LIMIT` | `u32` | 1 | 50 | **20** | Default pagination page size |
| `MAX_PAGE_LIMIT` | `u32` | 20 | — | **50** | Max pagination page size |

### Storage-Backed Parameters (set at runtime)

| Parameter | Type | Min | Max | Default | Set By | Description |
| --- | --- | --- | --- | --- | --- | --- |
| PauseAdmin | `Address` | — | — | None (first caller) | `set_pause_admin` | Who can pause/unpause savings |
| UpgradeAdmin | `Address` | — | — | None (first caller) | `set_upgrade_admin` | Who can upgrade savings contract |
| `Paused` (global) | `bool` | — | — | `false` | `pause` / `unpause` (pause admin) | Global pause state |
| `PausedFunctions` | `Map<Symbol,bool>` | — | — | empty | `pause` / `unpause` (pause admin) | Function-level pause flags |

---

## 7. orchestrator

Defined in `orchestrator/src/lib.rs`.

### Hardcoded Constants (changeable only via upgrade)

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `MAX_USED_NONCES_PER_ADDR` | `u32` | — | — | **256** | Max tracked nonces per address |
| `MAX_DEADLINE_WINDOW_SECS` | `u64` | — | — | **3_600 (1 h)** | Max validity window for signed requests (seconds) |
| `MAX_AUDIT_ENTRIES` | `u32` | — | — | **100** | Max audit entries in ring buffer |

### Storage-Backed Parameters (set at runtime)

| Parameter | Type | Min | Max | Default | Set By | Description |
| --- | --- | --- | --- | --- | --- | --- |
| Owner | `Address` | — | — | Set in `init` | `initialize` (deployer) | Orchestrator contract owner |
| Dependencies (5 addresses) | `Address` each | — | — | Set in `init` | `initialize` (deployer) | family_wallet, remittance_split, savings_goals, bill_payments, insurance addresses |

---

## 8. reporting

Defined in `reporting/src/lib.rs`.

### Hardcoded Constants (changeable only via upgrade)

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `MAX_DEP_PAGES` | `u32` | — | — | **20** | Max pages fetched from any single dependency per report |
| `DEP_PAGE_LIMIT` | `u32` | — | — | **50** | Page size for dependency queries |
| `MAX_ITEMS_PER_REPORT` | `u32` | — | — | **10** | Max items in top-N reports |

### Storage-Backed Parameters (set at runtime)

| Parameter | Type | Min | Max | Default | Set By | Description |
| --- | --- | --- | --- | --- | --- | --- |
| Admin | `Address` | — | — | Set in `init` | `initialize` (deployer) | Who can configure addresses, archive reports |
| ContractAddresses (5 deps) | `Address` each | — | — | Set via `configure_addresses` | `configure_addresses` (admin) | Dependency contract addresses |

---

## 9. emergency_killswitch

Defined in `emergency_killswitch/src/lib.rs`.

### Hardcoded Constants (changeable only via upgrade)

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `MAX_PAUSED_FUNCTIONS` | `u32` | — | — | **10** | Max functions in a single module's pause list |

### Storage-Backed Parameters (set at runtime)

| Parameter | Type | Min | Max | Default | Set By | Description |
| --- | --- | --- | --- | --- | --- | --- |
| Admin | `Address` | — | — | Set in `initialize` | `initialize` / `transfer_admin` | Single global killswitch admin |

---

## 10. data_migration

Defined in `data_migration/src/lib.rs`. All are hardcoded — changeable only via upgrade.

### Hardcoded Constants

| Parameter | Type | Min | Max | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `SCHEMA_VERSION` | `u32` | — | — | **1** | Current snapshot schema version |
| `MIN_SUPPORTED_VERSION` | `u32` | — | — | **1** | Min supported snapshot version for import |
| `MIN_SUPPORTED_ENCRYPTED_VERSION` | `u32` | — | — | **1** | Min supported encrypted payload version |
| `MAX_SUPPORTED_ENCRYPTED_VERSION` | `u32` | — | — | **2** | Max supported encrypted payload version |
| `MAX_MIGRATION_PAYLOAD_BYTES` | `usize` | — | — | **65_536 (64 KB)** | Max plaintext migration payload size |
| `MAX_MIGRATION_RECORDS` | `usize` | — | — | **1_024** | Max records per migration snapshot |
| `MAX_MIGRATION_SNAPSHOT_BYTES` | `usize` | — | — | **98_304 (96 KB)** | Max serialized snapshot size |

---

## 11. Cross-Cutting Admin Roles

Each contract stores admin addresses in instance storage and exposes entrypoints to transfer them.

### Role Inventory

| Role | Storage Key | Set By | Used In |
| --- | --- | --- | --- |
| Pause Admin | `symbol_short!("PAUSE_ADM")` | Owner (initial), current Pause Admin (subsequent) | `bill_payments`, `remittance_split`, `savings_goals`, `family_wallet` |
| Upgrade Admin | `symbol_short!("UPG_ADM")` | Owner (initial), current Upgrade Admin (subsequent) | `bill_payments`, `remittance_split`, `savings_goals`, `family_wallet` |
| Contract Owner | `DataKey::Owner` / `symbol_short!("OWNER")` | Set during `init` | `insurance`, `remittance_split`, `orchestrator`, `family_wallet` |
| Killswitch Admin | `DataKey::Admin` | Set during `initialize`, then via `transfer_admin` | `emergency_killswitch` |
| Reporting Admin | `symbol_short!("ADMIN")` | Set during `init` | `reporting` |

### Governance Entry Points

| Entrypoint | Auth Guard | Sets |
| --- | --- | --- |
| `configure_multisig` | `is_owner_or_admin` | `MultiSigConfig` (threshold, signers, spending_limit) per tx type |
| `update_spending_limit` | `require_governance_ok` | Per-member spending limit |
| `configure_emergency` | `is_owner_or_admin` | `EmergencyConfig` (max_amount, cooldown, min_balance, daily_limit) |
| `set_proposal_expiry` | Owner only | Proposal expiry window |
| `set_role_expiry` | Admin | Per-member role expiry |
| `set_precision_spending_limit` | `is_owner_or_admin` | Per-member `PrecisionSpendingLimit` |
| `set_pause_admin` | Owner / current pause admin | Pause admin address |
| `set_upgrade_admin` | Owner / current upgrade admin | Upgrade admin address |
| `pause` / `unpause` | Pause admin | Global or function-level pause state |
| `update_split` | Split owner | Split percentages (spending, savings, bills, insurance) |
| `set_external_ref` (bills/insurance) | Owner or bill/policy owner | Bill/policy external reference |
| `deactivate_policy` | Policy or contract owner | Policy active status |
| `set_time_lock` (savings) | Goal owner | Goal unlock date |
| `configure_addresses` (reporting) | Admin | Dependency contract addresses |
| `transfer_admin` (killswitch) | Admin | Killswitch admin address |
