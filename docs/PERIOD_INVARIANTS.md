# Period Invariants: Time-Bound Mechanics Across Contracts

## Audience

This document is for **contributors** (engineers creating, modifying, or reviewing Soroban smart contracts in `Remitwise-Contracts`). It specifies the rules and invariants governing time periods, ledger timestamp checks, deadline windows, expiration mechanics, and recurring schedule calculations across all contract crates.

---

## Period Invariant Principles

All contracts in this workspace operate under `#![no_std]` constraints and rely strictly on Soroban SDK environment primitives:

1. **Ledger Time Authority:** `env.ledger().timestamp()` is the sole authoritative clock. Contracts NEVER accept unverified client-side timestamps for period calculations.
2. **Strict Period Boundaries:** All period comparisons are strict inequalities or explicit boundary checks ($T_{\text{current}} \ge T_{\text{target}}$) to prevent premature execution or race conditions.
3. **No Overflow Invariant:** Period additions (`timestamp + duration`) must use checked arithmetic (`checked_add`) or safe integer conversions to prevent integer overflow panics on Soroban runtime.
4. **Deterministic Duration Units:** All durations in parameters and storage are expressed in **seconds** ($1 \text{ day} = 86,400 \text{ seconds}$, $1 \text{ week} = 604,800 \text{ seconds}$, $30 \text{ days} = 2,592,000 \text{ seconds}$).

---

## Per-Crate Period Invariants

| Crate | Period Mechanism | Primary Invariant | Storage / Param | Failure Error |
| :--- | :--- | :--- | :--- | :--- |
| `remittance_split` | Schedule Execution Window | `next_run_at > last_run_at`, execution permitted iff `env.ledger().timestamp() >= next_run_at` | `ScheduleKey` / `next_run_at` | `RemittanceSplitError::ScheduleNotDue` |
| `insurance` | Premium Payment Grace Window | Premium payment accepted iff `env.ledger().timestamp() >= next_due_date - GRACE_PERIOD` | `PolicyData` / `next_due_date` | `InsuranceError::PaymentNotDue` |
| `family_wallet` | Role & Multisig Expiry | Action rejected if `role_expiry > 0` and `env.ledger().timestamp() > role_expiry` | `RoleKey` / `role_expiry` | `FamilyWalletError::RoleExpired` |
| `orchestrator` | Execution Deadline Window | Flow execution rejected if `env.ledger().timestamp() > deadline` | `ExecutionParams` / `deadline` | `OrchestratorError::DeadlineExceeded` |
| `bill_payments` | Bill Due Date & Grace Period | Bill marked overdue if `env.ledger().timestamp() > due_date` | `BillData` / `due_date` | `BillPaymentsError::BillOverdue` |
| `emergency_killswitch` | Timelock Cooling Window | Action executable only after `env.ledger().timestamp() >= timelock_until` | `KillswitchData` / `timelock_until` | `KillswitchError::TimelockActive` |

---

## Detailed Contract Specifications

### 1. Remittance Split Schedule Execution (`remittance_split`)

Recurring remittance schedules allow periodic automatic transfers.

#### Invariants
- **Execution Eligibility:** A schedule cannot be executed before `next_run_at`.
- **Interval Advance:** Upon successful execution, `next_run_at` is updated to:
  $$\text{next\_run\_at}_{\text{new}} = \text{env.ledger().timestamp()} + \text{frequency\_seconds}$$
- **Double Execution Prevention:** Multiple executions within the same period window revert with `ScheduleNotDue`.

#### Concrete Rust Code Example

```rust
use soroban_sdk::{env, Env, Symbol};

pub fn execute_scheduled_split(env: Env, schedule_id: u64) -> Result<(), RemittanceSplitError> {
    let mut schedule = get_schedule(&env, schedule_id)?;
    let current_time = env.ledger().timestamp();

    if current_time < schedule.next_run_at {
        return Err(RemittanceSplitError::ScheduleNotDue);
    }

    // Execute split logic...

    schedule.last_run_at = current_time;
    schedule.next_run_at = current_time.checked_add(schedule.frequency_seconds)
        .ok_or(RemittanceSplitError::Overflow)?;

    set_schedule(&env, schedule_id, &schedule);
    Ok(())
}
```

---

### 2. Micro-Insurance Premium Payment Windows (`insurance`)

Policies require periodic premium payments to maintain active coverage.

#### Invariants
- **Payment Window:** Payments are permitted within the active window $[ \text{next\_due\_date} - \text{PREMIUM\_WINDOW}, \text{next\_due\_date} + \text{GRACE\_PERIOD} ]$.
- **Early Payment Rejection:** Attempts to pay before the payment window opens revert with `PaymentNotDue`.
- **Coverage Expiry:** If current timestamp exceeds $\text{next\_due\_date} + \text{GRACE\_PERIOD}$, the policy transitions to `Overdue`/`Lapsed` state.

#### Concrete Rust Code Example

```rust
const PREMIUM_WINDOW_SECONDS: u64 = 259_200; // 3 days

pub fn pay_premium(env: Env, policy_id: u64) -> Result<(), InsuranceError> {
    let mut policy = get_policy(&env, policy_id)?;
    let now = env.ledger().timestamp();

    if now + PREMIUM_WINDOW_SECONDS < policy.next_due_date {
        return Err(InsuranceError::PaymentNotDue);
    }

    // Process premium transfer...

    policy.next_due_date = policy.next_due_date.checked_add(policy.period_duration)
        .ok_or(InsuranceError::Overflow)?;

    save_policy(&env, policy_id, &policy);
    Ok(())
}
```

---

### 3. Family Wallet Role & Proposal Expiry (`family_wallet`)

Sub-account permissions and multisig proposals are constrained by expiration timestamps.

#### Invariants
- **Role Expiry Guard:** If `role_expiry > 0` and `env.ledger().timestamp() >= role_expiry`, any action attempted by the member reverts with `RoleExpired`.
- **Proposal Expiry Guard:** Multisig approval proposals expire if current ledger timestamp exceeds `proposal.created_at + PROPOSAL_TTL`.

---

### 4. Orchestrator Deadline Windows (`orchestrator`)

Orchestrator multi-call flows specify a hard transaction execution deadline.

#### Invariants
- **Hard Execution Deadline:** If `env.ledger().timestamp() > deadline`, the transaction immediately reverts with `DeadlineExceeded`.
- **Atomic Reversal:** Expired flows leave all contract states unmodified.

```rust
pub fn execute_flow(env: Env, deadline: u64) -> Result<(), OrchestratorError> {
    if env.ledger().timestamp() > deadline {
        return Err(OrchestratorError::DeadlineExceeded);
    }
    // Perform multi-contract orchestration...
    Ok(())
}
```

---

## Contributor Verification Checklist

When adding or modifying period-tied logic:
- [ ] Uses `env.ledger().timestamp()` exclusively for time queries.
- [ ] Uses `checked_add` / `checked_sub` for all timestamp calculations.
- [ ] Expresses all durations in seconds ($u64$).
- [ ] Returns explicit, named error variants (`ScheduleNotDue`, `DeadlineExceeded`, `PaymentNotDue`, `RoleExpired`).
- [ ] Covers boundary conditions ($T - 1\text{s}$, $T$, $T + 1\text{s}$) in unit tests using `env.ledger().set_timestamp(...)`.

## Shared Period-Active Guard

`remitwise-common::verify_period_active(period_start, now, is_archived)` is
the canonical helper for rejecting writes against periods that are either
**future** (`period_start > now`) or **archived** (`is_archived == true`). Call
it at the top of any write entry point that takes a `(user, period_key)`
composite key or any storage layout that partitions by period, before the
write mutates storage. It returns
`Err(remitwise_common::PeriodKeyError::PeriodNotActive)` on any rejection,
which the call site maps to its own contract-specific `#[contracterror]`.

**Rationale.** Without this guard, a buggy or compromised caller could
pre-load a future period with self-serving state (gaming month-end scoring
reports), or resurrect state under a `(user, pk)` composite whose period
has already been sealed into the archive map — breaking the invariant that
the archive map is immutable once sealed. The helper is pure and
stateless; the caller supplies `is_archived` from its own archive
tracking. See [`remitwise-common/src/period.rs`](../remitwise-common/src/period.rs)
and [`docs/PERIOD_KEYS.md`](PERIOD_KEYS.md) for the full specification.
