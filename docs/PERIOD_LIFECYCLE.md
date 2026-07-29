# Period Lifecycle & State Machine

This document describes the lifecycle, state transitions, state guards, and invariant rules for accounting and settlement periods within the **RemitWise Smart Contracts** workspace.

---

## Audience

This document is written for **contributors** developing, reviewing, or integrating Soroban smart contracts that rely on time-bounded financial or settlement periods.

---

## Overview

A **Period** represents a bounded time window (e.g., daily, weekly, or monthly cycle) during which remittances, bill payments, micro-insurance premiums, and family wallet disbursements are executed and reconciled. Every period progresses through four distinct lifecycle states:

$$\text{Open} \longrightarrow \text{Active} \longrightarrow \text{Closing} \longrightarrow \text{Archived}$$

---

## Lifecycle States

| State | Enum Discriminant | Description | Operations Allowed | State Mutations |
|---|---|---|---|---|
| **`Open`** | `0` | Initialized future period ready for pre-scheduling. | Draft schedules, register recurring transfers. | Configuration updates allowed |
| **`Active`** | `1` | Current operational period in active execution. | Full transfer processing, split execution, bill payments. | Full execution allowed |
| **`Closing`** | `2` | Settlement & reconciliation phase. | Finalize pending in-flight transactions, process adjustments. | Settlement completions only (no new entries) |
| **`Archived`** | `3` | Fully reconciled historical record. | Read-only range queries, audit trail verification. | None (Strictly Immutable) |

---

## State Machine Diagram

```
                 initialize
                     │
                     ▼
             ┌───────────────┐
             │     Open      │  (Discriminant = 0)
             └───────┬───────┘
                     │ activate
                     ▼
             ┌───────────────┐
             │    Active     │  (Discriminant = 1)
             └───────┬───────┘
                     │ start_close
                     ▼
             ┌───────────────┐
             │    Closing    │  (Discriminant = 2)
             └───────┬───────┘
                     │ archive
                     ▼
             ┌───────────────┐
             │   Archived    │  (Discriminant = 3, Terminal & Immutable)
             └───────────────┘
```

---

## Transition Rules & Invariants

1. **Sequential Progression:** Transitions must follow the strict sequence: `Open` $\rightarrow$ `Active` $\rightarrow$ `Closing` $\rightarrow$ `Archived`.
2. **No Skipping States:** Direct transitions (e.g., `Open` $\rightarrow$ `Closing` or `Active` $\rightarrow$ `Archived`) are rejected.
3. **No Re-opening:** Once a period enters `Closing` or `Archived`, it can never transition back to `Open` or `Active`.
4. **New Entry Guard:** Functions initiating new financial transactions must verify that the target period is currently in the `Active` state.
5. **Reconciliation Lock:** During `Closing`, only settlement completion calls for previously initiated transfers are permitted. New allocation or transfer requests are rejected.
6. **#![no_std] Discipline:** All contract types and state enums use `soroban_sdk` primitives and `#[contracttype]` macro annotations without standard library `std` calls.

---

## Concrete Soroban Contract Implementation

### 1. Data Types (`remitwise-common`)

```rust
use soroban_sdk::{contracttype, Symbol, Env};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PeriodState {
    Open = 0,
    Active = 1,
    Closing = 2,
    Archived = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Period {
    pub id: Symbol,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub state: PeriodState,
}
```

### 2. State Guard Helper

```rust
use soroban_sdk::{Env, Symbol};

pub fn require_period_state(
    env: &Env,
    current_state: PeriodState,
    expected_state: PeriodState,
) -> Result<(), PeriodError> {
    if current_state != expected_state {
        return Err(PeriodError::InvalidState);
    }
    Ok(())
}

pub fn validate_state_transition(
    from: PeriodState,
    to: PeriodState,
) -> Result<(), PeriodError> {
    match (from, to) {
        (PeriodState::Open, PeriodState::Active) => Ok(()),
        (PeriodState::Active, PeriodState::Closing) => Ok(()),
        (PeriodState::Closing, PeriodState::Archived) => Ok(()),
        _ => Err(PeriodError::InvalidStateTransition),
    }
}
```

### 3. Entrypoint Example: Period Transition

```rust
use soroban_sdk::{contractimpl, Address, Env, Symbol};

pub struct PeriodContract;

#[contractimpl]
impl PeriodContract {
    /// Transition a period to its next lifecycle state.
    /// Requires admin authorization.
    pub fn transition_period(
        env: Env,
        admin: Address,
        period_id: Symbol,
        target_state: PeriodState,
    ) -> Result<(), PeriodError> {
        admin.require_auth();

        let mut period: Period = env
            .storage()
            .persistent()
            .get(&period_id)
            .ok_or(PeriodError::NotFound)?;

        // Enforce valid sequential transition
        validate_state_transition(period.state, target_state)?;

        period.state = target_state;
        env.storage().persistent().set(&period_id, &period);

        // Emit standardized lifecycle transition event
        env.events().publish(
            (Symbol::new(&env, "period"), Symbol::new(&env, "transition")),
            (period_id, target_state),
        );

        Ok(())
    }
}
```

---

## Related Documentation

- [Period Range Validation Helper](validate-period.md)
- [Storage Layout & TTL Standards](../STORAGE_LAYOUT.md)
- [Architecture Overview](../ARCHITECTURE.md)
- [Contributor Overview](CONTRIBUTOR_OVERVIEW.md)
