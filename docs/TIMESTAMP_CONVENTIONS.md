# Timestamp Conventions

## Audience

This document is for **contributors** who create, modify, or review Soroban smart contracts
in `Remitwise-Contracts`. It specifies how time is represented, stored, and compared across
all contract crates, and provides concrete examples that reviewers can verify against the
documented intent.

---

## Core Principle

The Soroban ledger timestamp is the **sole authoritative clock**. Contracts MUST NOT accept
unverified client-supplied timestamps for any time-sensitive logic.

---

## Representation

| Aspect | Convention |
|---|---|
| **Type** | `u64` |
| **Epoch** | Unix epoch (1970-01-01 00:00:00 UTC) |
| **Unit** | Seconds |
| **Source** | `env.ledger().timestamp()` |

All `timestamp` fields in storage structs and event payloads are `u64` values holding
seconds since the Unix epoch. No sub-second precision is used.

### Storage Structs

| Struct | Field | File |
|---|---|---|
| `PauseState` | `timestamp: u64` | `remitwise-common/src/lib.rs:121` |
| `LimitSnapshot` | `timestamp: u64` | `remitwise-common/src/lib.rs:129` |
| `PolicyData` | `timestamp: u64` | `insurance/src/lib.rs:202` |
| `BillData` | `timestamp: u64` | `bill_payments/src/lib.rs:118` |
| `GoalData` | `timestamp: u64` | `family_wallet/src/lib.rs:220` |

---

## Accessing the Current Time

Every contract accesses the ledger clock through `env.ledger().timestamp()`. There is
no alternative clock source.

```rust
let now = env.ledger().timestamp();
```

The Soroban host guarantees that `timestamp` never decreases within a single
transaction and strictly increases across ledgers
([see `LEDGER_MONOTONICITY.md`](docs/LEDGER_MONOTONICITY.md)).

---

## Conventions

### 1. Age Calculation — `saturating_sub`

When computing how long ago something happened, always use `saturating_sub` to avoid
underflow if the stored timestamp is in the future (which can occur due to clock drift
or migration edge cases):

```rust
// remitwise-common/src/lib.rs:433
let age = env.ledger().timestamp().saturating_sub(snapshot_taken_at);
```

### 2. Deadline Comparison — `now >= deadline` means expired

A stored deadline is **expired** when the current ledger timestamp has reached or
passed it. Use `>=` for "deadline has passed" checks:

```rust
// insurance/src/lib.rs:574
let now = env.ledger().timestamp();
if now >= policy.next_due_date {
    // premium is overdue
}
```

### 3. Future Timestamp Guard

When a function requires a timestamp to be in the future, compare `now` against the
provided value and reject if `now >= target`:

```rust
// bill_payments/src/lib.rs:990
pub fn schedule_unpause(env: Env, caller: Address, at_timestamp: u64) -> Result<(), Error> {
    // ...
    if at_timestamp <= env.ledger().timestamp() {
        // reject: timestamp must be in the future
    }
}
```

### 4. Period Bucketing — `Timestamp::to_period_key`

The `Timestamp` helper in `remitwise-common` provides `to_period_key` which converts
a `u64` Unix timestamp into a stable period identifier:

| Period | Bucket | Example (`timestamp = 1_700_000_000`) |
|---|---|---|
| `Day` | `timestamp / 86400` | `19652` |
| `Week` | `timestamp / 604800` | `2816` |
| `Month` | `YYYYMM` integer | `202311` |

```rust
use remitwise_common::Timestamp;
use remitwise_common::PeriodKind;

let day_key = Timestamp::to_period_key(now, PeriodKind::Day);
let month_key = Timestamp::to_period_key(now, PeriodKind::Month);
```

This is used in `bill_payments`, `insurance`, and `reporting` for daily and monthly
aggregation keys.

---

## Testing with Mocked Time

In unit tests, advance the mock ledger clock with `env.ledger().set_timestamp(...)`:

```rust
// insurance/src/lib.rs:1738
env.ledger().set_timestamp(BASE_TIME);
```

Test patterns for clock-dependent logic:

1. **Boundary conditions** — test at `T - 1`, `T`, and `T + 1` where `T` is a
   deadline or threshold.
2. **Time progression** — call `set_timestamp` to advance the clock between
   operations and verify state transitions fire correctly.
3. **Expiration** — store a timestamp, advance the clock past it, and assert the
   entrypoint rejects the operation or transitions state.

---

## The `Timestamp` Helper (remitwise-common)

The shared crate provides two helpers:

| Helper | Signature | Purpose |
|---|---|---|
| `Timestamp::seconds_until` | `seconds_until(now: u64, target: u64) -> u64` | Returns `target.saturating_sub(now)` |
| `Timestamp::to_period_key` | `to_period_key(timestamp: u64, period: PeriodKind) -> u64` | Buckets a Unix timestamp into a day/week/month key |

Both are `#[inline(always)]` and never panic on any `u64` input.

### `seconds_until` Example

```rust
use remitwise_common::Timestamp;

let remaining = Timestamp::seconds_until(env.ledger().timestamp(), pause_until);
// remaining == 0 when the pause has expired
```

---

## Summary of Rules

| Rule | Rationale |
|---|---|
| Use `env.ledger().timestamp()` exclusively | Host provides the only trusted clock |
| Store timestamps as `u64` (Unix epoch seconds) | Consistent representation across all contracts |
| Use `saturating_sub` for age/difference calculations | Prevent underflow on future-drifted timestamps |
| Compare `now >= deadline` for expiry checks | Clear, unambiguous semantics |
| Use `Timestamp::to_period_key` for bucketing | Deterministic, timezone-independent, no allocation |
| Test with `env.ledger().set_timestamp(...)` | Controlled time advancement in mock environment |
| Never accept client-supplied timestamps for critical logic | Prevents timestamp manipulation attacks |