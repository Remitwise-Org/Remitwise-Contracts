# Ledger Monotonicity

## Audience

This document is for **contributors** who need to understand where and why the
RemitWise contracts rely on ledger monotonicity. Knowing these sites helps
reviewers verify that new code respects the same guarantees and helps
contributors avoid introducing time-based or sequence-based bugs.

## What Ledger Monotonicity Means

The Soroban host guarantees two monotonicity properties for every contract
invocation:

| Property | Guarantee | Accessor |
|---|---|---|
| **Sequence** | Strictly increases by 1 every ledger; never decreases | `env.ledger().sequence()` |
| **Timestamp** | Never decreases within the same transaction; increases across ledgers | `env.ledger().timestamp()` |

These are **runtime invariants** of the Soroban environment, not something the
contract enforces itself. The contract code *relies* on these guarantees to
implement safety-critical logic:

- A schedule's `next_due` comparison against `env.ledger().timestamp()` is only
  meaningful because the ledger clock never runs backward.
- A `last_executed` guard prevents double-execution only if the ledger
  timestamp cannot repeat within the same execution context.
- A stored `PADM_GT` (admin grant timestamp) expiry check is only safe because
  the ledger clock advances monotonically.

## Enforcement Sites

### 1. Time-Lock Forward-Only (`savings_goals`)

When a savings goal has an active time-lock (`unlock_date > current_time`),
`set_time_lock` prevents shortening the unlock date — it may only be extended
forward.

| | |
|---|---|
| **File** | `savings_goals/src/lib.rs` |
| **Function** | `set_time_lock` |
| **Lines** | 2433–2509 |
| **Guard** | `if prev_unlock > current_time && unlock_date < prev_unlock` |
| **Error** | `SavingsGoalError::TimeLockShortening` |

```rust
/// # Monotonicity rule (forward-only)
/// While a time-lock is active (i.e. `unlock_date` is set to a timestamp
/// strictly greater than the current ledger time), `set_time_lock` **may
/// only move `unlock_date` forward** (extend), never backward (shorten).
pub fn set_time_lock(env: Env, caller: Address, goal_id: u32, unlock_date: u64) -> bool {
    let current_time = env.ledger().timestamp();
    if unlock_date <= current_time {
        panic!("Unlock date must be in the future");
    }
    if let Some(prev_unlock) = goal.unlock_date {
        if prev_unlock > current_time && unlock_date < prev_unlock {
            soroban_sdk::panic_with_error!(env, SavingsGoalError::TimeLockShortening);
        }
    }
    // ...
}
```

**Why it matters:** Without this guard, a compromised or buggy caller could
lock funds for a shorter duration than the user agreed to, undermining the
time-lock's purpose as a forward-only commitment.

---

### 2. Schedule Execution Idempotency (bill_payments, insurance, remittance_split, savings_goals)

All four schedule-based contracts follow the same idempotency pattern: compare
`last_executed` against `next_due` to skip re-execution, then advance
`next_due` past any missed periods using the ledger timestamp.

| | |
|---|---|
| **Files** | `bill_payments/src/lib.rs`, `insurance/src/lib.rs`, `remittance_split/src/lib.rs`, `savings_goals/src/lib.rs` |
| **Functions** | `execute_due_schedules`, `execute_due_premium_schedules`, `execute_due_remittance_schedules`, `execute_due_savings_schedules` |
| **Guard** | `if last_exec >= schedule.next_due { continue; }` |
| **Advancement** | `while next <= current_time { missed += 1; next += interval; }` |

```rust
// Canonical pattern (bill_payments/src/lib.rs ~line 1585)
if let Some(last_exec) = schedule.last_executed {
    if last_exec >= schedule.next_due {
        continue; // already executed for this period
    }
}

// Advance next_due past any missed periods
let mut next = schedule.next_due;
let mut missed = 0u64;
while next <= current_time {
    missed += 1;
    next = next.saturating_add(schedule.interval);
}
schedule.next_due = next;

// Only record the execution once
schedule.last_executed = Some(current_time);
```

**Why it matters:** Without the `last_exec >= next_due` guard, the same
schedule could execute multiple times within the same ledger (duplicate
transfers). Without the monotonic timestamp, the advancement loop could loop
forever or skip incorrectly.

---

### 3. Due Date Freshness

Every schedule creation entrypoint validates that `next_due` is strictly
greater than the current ledger timestamp.

| Contract | Entrypoints | Error |
|---|---|---|
| `bill_payments` | `create_bill_schedule`, `modify_bill_schedule`, `create_bill` | `InvalidDueDate` |
| `insurance` | `create_premium_schedule`, `modify_premium_schedule` | `InvalidDueDate` |
| `remittance_split` | `create_remittance_schedule`, `modify_remittance_schedule` | `InvalidDueDate` |
| `savings_goals` | `create_savings_schedule`, `modify_savings_schedule` | `InvalidDueDate` |

```rust
// Canonical pattern (insurance/src/lib.rs ~line 1125)
let now = env.ledger().timestamp();
if next_due <= now {
    return Err(Error::InvalidDueDate);
}
```

**Why it matters:** Without this check, a schedule could be created with a due
date in the past and execute immediately on the first call to
`execute_due_*_schedules`, bypassing any intended delay.

---

### 4. Snapshot Freshness (6 contracts, via `remitwise-common`)

The `require_recent_snapshot` helper in `remitwise-common` uses the ledger
timestamp to reject stale snapshots.

| | |
|---|---|
| **File** | `remitwise-common/src/lib.rs` |
| **Function** | `require_recent_snapshot` |
| **Lines** | 554–562 |
| **Guard** | `age > SNAPSHOT_MAX_AGE_SECS` |

```rust
pub fn require_recent_snapshot(env: &Env, snapshot_taken_at: u64) -> Result<(), SnapshotError> {
    let age = env.ledger().timestamp().saturating_sub(snapshot_taken_at);
    if age > SNAPSHOT_MAX_AGE_SECS {
        Err(SnapshotError::SnapshotTooOld)
    } else {
        Ok(())
    }
}
```

Called by: `savings_goals`, `bill_payments`, `insurance`, `remittance_split`,
`family_wallet`, `orchestrator`.

**Why it matters:** A stale snapshot could restore state that has since been
overwritten, causing funds to double-count or go missing.

---

### 5. Admin Grant TTL (`bill_payments`)

The pause admin grant (`PADM_GT`) records the timestamp when the grant was
issued and checks it against the current ledger timestamp.

| | |
|---|---|
| **File** | `bill_payments/src/lib.rs` |
| **Function** | `require_admin_grant_valid` |
| **Lines** | 813–833 |
| **Guard** | `now >= granted_at.saturating_add(ADMIN_GRANT_TTL)` |
| **Error** | `BillPaymentsError::AdminGrantExpired` |

```rust
fn require_admin_grant_valid(env: &Env) -> Result<(), BillPaymentsError> {
    let granted_at: Option<u64> = env.storage().instance().get(&symbol_short!("PADM_GT"));
    match granted_at {
        Some(granted) => {
            let now = env.ledger().timestamp();
            if now >= granted.saturating_add(ADMIN_GRANT_TTL) {
                Err(BillPaymentsError::AdminGrantExpired)
            } else { Ok(()) }
        }
        None => Err(BillPaymentsError::AdminGrantExpired),
    }
}
```

**Why it matters:** Without monotonic time, the grant expiry check could
produce false negatives/positives, allowing an expired admin to retain
control.

---

### 6. Unpause Time-locks (`bill_payments`, `emergency_killswitch`)

Both contracts support scheduling an unpause at a future timestamp.

| Contract | Function | Guard | Error |
|---|---|---|---|
| `bill_payments` | `schedule_unpause` | `if at_timestamp <= env.ledger().timestamp()` | `InvalidSchedule` |
| `emergency_killswitch` | `schedule_unpause` | `if time < env.ledger().timestamp()` | `InvalidSchedule` |
| `emergency_killswitch` | `unpause` | `if env.ledger().timestamp() < schedule` | `Unauthorized` |

**Why it matters:** A scheduled unpause must be guaranteed to activate no
earlier than its configured time. Without monotonicity, the guard is
meaningless.

---

### 7. Role Expiry (`family_wallet`)

Member roles have an optional expiry timestamp.

| | |
|---|---|
| **File** | `family_wallet/src/lib.rs` |
| **Function** | `role_has_expired` |
| **Lines** | 3063–3068 |
| **Guard** | `env.ledger().timestamp() >= expires_at` |

```rust
fn role_has_expired(env: &Env, address: &Address) -> bool {
    if let Some(exp) = Self::get_role_expiry(env, address) {
        env.ledger().timestamp() >= exp
    } else {
        false
    }
}
```

**Why it matters:** Expiry is inclusive — at `ledger.timestamp() >= expires_at`
the member is treated as expired. A non-monotonic clock could resurrect an
expired member's access.

---

### 8. Ledger Sequence Matching (`remitwise-common` — defined but unused)

The `require_matching_ledger` helper in `remitwise-common` compares the current
ledger sequence against an expected value. It was designed as a replay-prevention
primitive for signed operations that commit to a specific ledger.

| | |
|---|---|
| **File** | `remitwise-common/src/lib.rs` |
| **Function** | `require_matching_ledger` |
| **Lines** | 910–931 |
| **Guard** | `current != expected` |
| **Error** | `LedgerError::LedgerMismatch` |

```rust
/// Asserts that `expected` matches the current ledger sequence number.
///
/// This is a replay-prevention helper: if an operation was authorized for a
/// specific ledger (e.g. via a signed nonce bound to a ledger), executing it in
/// a different ledger would let an attacker replay the same authorization in a
/// later ledger.
pub fn require_matching_ledger(env: &Env, expected: u32) -> Result<(), LedgerError> {
    let current = env.ledger().sequence();
    if current != expected {
        Err(LedgerError::LedgerMismatch)
    } else {
        Ok(())
    }
}
```

**Status:** Defined but currently **not called** from any production
entrypoint. A future contributor adding replay-bound signed operations should
consider using this helper rather than reimplementing a ledger-sequence check.

---

### 9. Ledger Sequence Monotonicity (`remitwise-common` — `require_ledger_seq_monotonic`)

The `require_ledger_seq_monotonic` helper is the defence-in-depth companion
to `require_matching_ledger`. Where `require_matching_ledger` enforces
**exact equality** (useful for replay-bound signed operations that commit
to a specific ledger), `require_ledger_seq_monotonic` enforces a **lower
bound** (`current >= prev`). The two together cover the full ledger-
sequence trust model.

| | |
|---|---|
| **File** | `remitwise-common/src/lib.rs` |
| **Function** | `require_ledger_seq_monotonic` |
| **Guard** | `current < prev` |
| **Error** | `LedgerError::LedgerSequenceRegression` |

```rust
/// Asserts that the current ledger sequence is greater than or equal to a
/// previously observed baseline (`prev`).
///
/// This is a defence-in-depth monotonicity guard. Returned regression is the
/// canonical signal that an off-by-N replay, a stale-storage baseline, or a
/// `u32` cast underflow has reached a write entry point.
pub fn require_ledger_seq_monotonic(env: &Env, prev: u32) -> Result<(), LedgerError> {
    let current = env.ledger().sequence();
    if current < prev {
        Err(LedgerError::LedgerSequenceRegression)
    } else {
        Ok(())
    }
}
```

**Threat model.** Without this guard, a caller-supplied (or
stale-storage) baseline that walks backwards past the host sequence
allows replay of any operation committed to `prev` (fee updates, role
grants, mint caps, etc.) at a lower observed ledger. Returns
`Err(LedgerError::LedgerSequenceRegression)` so that the entry point can
map it to a contract-specific `#[contracterror]` rather than panicking.

**Why it matters:** Tying the cached baseline to `env.ledger().sequence()`
catches both replay attacks and any logic bug where a `u32` cast would
otherwise allow storage-baseline underflow to bypass the host invariant.

---

### 10. Cursor Monotonicity (`reporting`)

The reporting crate's dependent-query pagination loop guarantees termination
via a page counter bound, implicitly relying on cursor monotonicity.

| | |
|---|---|
| **File** | `reporting/src/lib.rs` |
| **Lines** | 525–532 |
| **Mechanism** | `pages_fetched` increment per iteration + `MAX_DEP_PAGES` cap |

Cursor monotonicity is guaranteed because every iteration increments
`pages_fetched` and the closure is the sole source of the next cursor; a buggy
closure that never returns 0 is still bounded by `MAX_DEP_PAGES`. This is a
loop-termination guarantee, not a ledger-level invariant, but it is documented
alongside the other monotonicity sites because reviewers often ask about it.

---

## Summary Table

| # | Mechanism | Crate | Relies on | Enforces |
|---|---|---|---|---|
| 1 | Time-lock forward-only | `savings_goals` | `timestamp()` monotonicity | `unlock_date` can only increase |
| 2 | Schedule execution idempotency | `bill_payments`, `insurance`, `remittance_split`, `savings_goals` | `timestamp()` monotonicity | No double-execution per period |
| 3 | Due date freshness | all 4 schedule contracts | `timestamp()` monotonicity | `next_due > now` at creation |
| 4 | Snapshot freshness | 6 contracts via `remitwise-common` | `timestamp()` monotonicity | Stale snapshot rejection |
| 5 | Admin grant TTL | `bill_payments` | `timestamp()` monotonicity | Grant expiry |
| 6 | Unpause timelocks | `bill_payments`, `emergency_killswitch` | `timestamp()` monotonicity | Scheduled unpause |
| 7 | Role expiry | `family_wallet` | `timestamp()` monotonicity | Expired member access |
| 8 | Ledger sequence matching | `remitwise-common` (unused) | `sequence()` monotonicity | Replay prevention |
| 9 | Cursor monotonicity | `reporting` | Loop counter | Pagination termination |

## Related Documentation

- [Contributor Overview](CONTRIBUTOR_OVERVIEW.md) — onboarding and development standards
- [Storage Layout Reference](../STORAGE_LAYOUT.md) — how timestamp/sequence values are persisted
- [Threat Model](../THREAT_MODEL.md) — security implications of time-based attacks
- [Amount Invariants](AMOUNT_INVARIANTS.md) — zero-handling across entrypoints
- [Committed Hashes](COMMITTED_HASHES.md) — request hashes and deadline model
