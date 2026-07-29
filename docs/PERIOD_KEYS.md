# Period Keys Specification: Model + Consumer Contract

This document provides the canonical specification for **Period Keys** (`period_key`) within the RemitWise smart contract workspace.

---

## Audience

This document is written for **downstream integrators** (frontend developers, indexers, analytics pipelines, off-chain reconciliation engines) and **contract contributors** interacting with or maintaining period-bound reporting and financial metrics in RemitWise contracts.

---

## 1. Overview & Purpose

In RemitWise, financial health scoring, spending history tracking, micro-insurance premium auditing, and recurring remittance analytics operate across discrete time windows. To scope state data cleanly without locking contracts to a rigid calendar schedule, contracts use a **Period Key** (`period_key`).

A `period_key` is a 64-bit unsigned integer (`u64`) that acts as a unique temporal identifier for a specific accounting or reporting window.

---

## 2. Model Contract (Data Structure & Types)

### 2.1 Primitive Type & Storage Representation

- **Type:** `u64` (native Soroban SDK integer primitive).
- **Storage Pattern (Composite Keying):**
  In contract instance storage (e.g., the `reporting` contract), active reports are stored in a Soroban `Map` using a composite tuple key:
  
  $$\text{Key}: (\text{Address}, \text{u64}) \longrightarrow \text{Value}: \text{FinancialHealthReport}$$

```rust
// Storage layout in reporting/src/lib.rs
// Map<(Address, u64), FinancialHealthReport>
let mut reports: Map<(Address, u64), FinancialHealthReport> = env
    .storage()
    .instance()
    .get(&symbol_short!("REPORTS"))
    .unwrap_or_else(|| Map::new(&env));
```

### 2.2 Domain Structs & Event Payload References

1. **`ArchivedReport` Struct (`reporting` crate):**
   ```rust
   #[contracttype]
   #[derive(Clone)]
   pub struct ArchivedReport {
       pub user: Address,
       pub period_key: u64,
       pub health_score: u32,
       pub generated_at: u64,
       pub archived_at: u64,
   }
   ```
2. **Financial History Tuples:**
   In reporting calculations, historical financial entries are represented as tuples of `(period_key, amount)`:
   ```rust
   // history: Vec<(u64, i128)>
   let history_entry: (u64, i128) = (202607u64, 150_0000000i128);
   ```
3. **Event Topic & Payload:**
   ```rust
   // Published upon report storage:
   env.events().publish(
       (symbol_short!("report"), ReportEvent::ReportStored),
       (user, period_key),
   );
   ```

### 2.3 Recommended Period Key Encoding Schemes

The `period_key` parameter is caller-defined to allow flexible integration with diverse business calendars. Downstream integrators must adopt a consistent encoding convention across their system.

| Calendar Granularity | Recommended Format | Format Description | Concrete Example (`period_key`) |
| :--- | :--- | :--- | :--- |
| **Monthly** | `YYYYMM` | Numeric Year + 2-digit Month | `202607` (July 2026) |
| **Daily** | `YYYYMMDD` | Numeric Year + Month + Day | `20260725` (July 25, 2026) |
| **Unix Epoch Window** | `u64` Timestamp | Start boundary timestamp in seconds | `1704067200` (2024-01-01T00:00:00Z) |

---

## 3. Consumer Contract (API Entrypoints & Guarantees)

### 3.1 Public Entrypoints (`reporting` Contract)

Downstream integrators interact with `period_key` primarily through the `reporting` contract entrypoints:

#### `store_report(env, user, report, period_key) -> bool`
- **Parameters:**
  - `user: Address`: Target account owner. Requires `user.require_auth()`.
  - `report: FinancialHealthReport`: Complete health report payload.
  - `period_key: u64`: Temporal key for the report.
- **Behavior:**
  - Validates `user` authorization.
  - Writes/overwrites the record under `(user, period_key)` in active instance storage.
  - Emits event topic `(symbol_short!("report"), ReportEvent::ReportStored)` with payload `(user, period_key)`.
  - Returns `true` on successful persistence.

#### `get_stored_report(env, _caller, user, period_key) -> Option<FinancialHealthReport>`
- **Parameters:**
  - `_caller: Address`: Caller address (reserved).
  - `user: Address`: Account owner to query. Requires `user.require_auth()`.
  - `period_key: u64`: Specific period key to look up.
- **Behavior:**
  - Returns `Some(FinancialHealthReport)` if an active report exists for `(user, period_key)`.
  - Returns `None` if no report has been submitted for that period key.

---

### 3.2 Guarantees & Constraints

1. **User Key Isolation:**
   Because storage keys are composite tuples `(user, period_key)`, User A cannot overwrite or access User B's report data, regardless of whether they pass identical `period_key` values.

2. **Idempotent Overwrites:**
   Calling `store_report` multiple times with the same `(user, period_key)` will update the report payload in-place for that period key, replacing the previous active report.

3. **Stateless Period Range Validation:**
   When verifying period boundaries (start and end timestamps associated with a `period_key`), contracts enforce `remitwise_common::validate_period(start, end)`, returning `Err(TimeError::InvalidPeriod)` if `start > end`.

4. **`#![no_std]` Strict Discipline:**
   All `period_key` logic relies strictly on `u64` primitives and `soroban_sdk` structures. No heap-allocated `std` time primitives or floating-point conversions are used.

---

## 4. Concrete Code Examples

### 4.1 Rust Client Interaction (Soroban SDK)

The following example demonstrates how a downstream service or integration test calls the `reporting` contract using Soroban SDK client bindings:

```rust
use soroban_sdk::{Env, Address, testutils::Address as _};
use reporting::{ReportingContractClient, FinancialHealthReport};

pub fn record_monthly_report(
    env: &Env,
    contract_id: &Address,
    user: &Address,
) {
    let client = ReportingContractClient::new(env, contract_id);

    // Construct a period key for July 2026 (YYYYMM format)
    let period_key: u64 = 202607;

    // Build report payload
    let report = FinancialHealthReport {
        overall_score: 85,
        savings_health: 90,
        bill_punctuality: 80,
        insurance_coverage: 85,
        generated_at: env.ledger().timestamp(),
    };

    // Store report under (user, period_key)
    let success = client.store_report(user, &report, &period_key);
    assert!(success);

    // Retrieve report by period_key
    let fetched = client.get_stored_report(user, user, &period_key);
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().overall_score, 85);
}
```

### 4.2 Indexer Event Subscription Pattern

Off-chain indexers monitoring the Stellar ledger parse `report` events to index reports by `period_key`:

```text
Event Topic 0: Symbol("report")
Event Topic 1: Symbol("ReportStored")
Event Data:    (Address(user), u64(period_key))
```

Indexers can query off-chain databases using `SELECT * FROM health_reports WHERE user_address = ? AND period_key = ?`.

---

## 5. Related Specifications

- **[docs/PERIOD_INVARIANTS.md](PERIOD_INVARIANTS.md)**: Rules governing `env.ledger().timestamp()`, deadline windows, and grace periods across contracts.
- **[docs/PERIOD_LIFECYCLE.md](PERIOD_LIFECYCLE.md)**: Four-state machine (`Open` $\rightarrow$ `Active` $\rightarrow$ `Closing` $\rightarrow$ `Archived`) for operational settlement periods.
- **[docs/validate-period.md](validate-period.md)**: Logical range validation utility (`validate_period`).
