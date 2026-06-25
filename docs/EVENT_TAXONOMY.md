# Event Taxonomy Documentation

## Overview

The **Remitwise** system uses a unified event taxonomy to categorize on‑chain events emitted by all contracts. This taxonomy is defined in `remitwise-common/src/lib.rs` and consists of two enums:

- `EventCategory` – high‑level classification of the event purpose.
- `EventPriority` – importance level for downstream processing.

These enums are encoded as `u32` values via the `to_u32()` methods and are included in the event topics using the pattern:

```rust
let topics = (
    symbol_short!("Remitwise"),
    category.to_u32(),
    priority.to_u32(),
    action,
);
```

Batch-summary events emitted through `RemitwiseEvents::emit_batch` use the same
category encoding, always use `EventPriority::Low`, and publish the fixed topic
tuple:

```rust
let topics = (
    symbol_short!("Remitwise"),
    category.to_u32(),
    EventPriority::Low.to_u32(),
    symbol_short!("batch"),
);
let data = (action, count);
```

The taxonomy enables off‑chain indexers and alerting services to reliably filter, sort, and interpret events across contracts.

### Event Data Size Budget

To prevent silent budget exhaustion, event data must be compact. The maximum recommended size for serialized event data is **256 bytes**. Emits should not contain bulk records, large collections (e.g., large `Vec`s), or extensive reporting structs. The primary payload should be small, typically containing only IDs, status codes, and small numerical amounts. A debug guard is enforced during testing to flag oversized payloads.

---

## Taxonomy Values

### EventCategory
| Variant | `to_u32()` Value | Description |
|--------|------------------|-------------|
| `Transaction` | `0` | Core business‑logic actions that move funds or change state. |
| `State` | `1` | Mutations to contract state that are not direct value transfers. |
| `Alert` | `2` | Signals intended for notification or monitoring systems (future alert consumer). |
| `System` | `3` | Lifecycle or administrative events (e.g., pausing, upgrades). |
| `Access` | `4` | Access‑control related events (member added, role changes). |

### EventPriority
| Variant | `to_u32()` Value | Description |
|--------|------------------|-------------|
| `Low` | `0` | Non‑critical events; safe to process asynchronously. |
| `Medium` | `1` | Important events that should be indexed promptly. |
| `High` | `2` | Critical events that may trigger alerts or require immediate attention. |

---

## Per‑Contract Event Mapping

Below is a representative mapping of events emitted by each contract to the taxonomy. The table lists the **event name**, the **enum variant** used for `EventCategory`, and the chosen **EventPriority**.

### Bill Payments (`bill_payments`)
| Event | Category | Priority |
|-------|----------|----------|
| `BillCreated` | `Transaction` | `Medium` |
| `BillPaid` | `Transaction` | `High` |
| `BillCancelled` | `State` | `Medium` |
| `BillsArchived` | `System` | `Low` |
| `BillRestored` | `State` | `Medium` |
| `ContractPaused/Unpaused` | `System` | `High` |
| `ContractUpgraded` | `System` | `High` |

### Savings Goals (`savings_goals`)
| Event | Category | Priority |
|-------|----------|----------|
| `GoalCreated` | `Transaction` | `Medium` |
| `FundsAdded` | `Transaction` | `Medium` |
| `GoalCompleted` | `Alert` | `High` |
| `FundsWithdrawn` | `Transaction` | `Medium` |
| `GoalLocked/Unlocked` | `Access` | `Low` |
| `ScheduleCreated` | `System` | `Low` |
| `ContractUpgraded` | `System` | `High` |

### Insurance (`insurance`)
| Event | Category | Priority |
|-------|----------|----------|
| `PolicyCreated` | `Transaction` | `Medium` |
| `PremiumPaid` | `Transaction` | `Low` |
| `PolicyDeactivated` | `State` | `Medium` |
| `ScheduleCreated` / `ScheduleExecuted` / `ScheduleMissed` | `System` | `Low` |
| `ContractUpgraded` | `System` | `High` |

### Remittance Split (`remittance_split`)
| Event | Category | Priority |
|-------|----------|----------|
| `SplitInitialized` | `Transaction` | `Medium` |
| `SplitCalculated` | `Transaction` | `Medium` |
| `ScheduleCreated` | `System` | `Low` |
| `ScheduleExecuted` | `System` | `Low` |
| `ScheduleMissed` | `System` | `Low` |
| `ContractPaused/Unpaused` | `System` | `High` |
| `ContractUpgraded` | `System` | `High` |

### Family Wallet (`family_wallet`)
| Event | Category | Priority |
|-------|----------|----------|
| `MemberAdded` | `Access` | `Low` |
| `SpendingLimitUpdated` | `Access` | `Low` |
| `TransactionProposed` | `Transaction` | `Medium` |
| `TransactionExecuted` | `Transaction` | `High` |
| `EmergencyModeOn/Off` | `System` | `High` |

### Orchestrator (`orchestrator`)
| Event | Category | Priority |
|-------|----------|----------|
| `FlowStarted` | `Transaction` | `High` |
| `FlowSucceeded` | `Transaction` | `High` |
| `FlowFailed` | `Alert` | `High` |
| `ContractPaused/Unpaused` | `System` | `High` |
| `ContractUpgraded` | `System` | `High` |

---

## Usage in Code

The `RemitwiseEvents::emit` and `emit_batch` helpers automatically construct the topic tuple using the enums' `to_u32()` values. See the updated documentation comments in `remitwise-common/src/lib.rs` for examples.

---

*Generated by Antigravity on 2026‑06‑18.*
