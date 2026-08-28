# Cross-Contract Ledger Time

## Audience

This document is for **contributors** who are writing or reviewing cross-contract flows in `Remitwise-Contracts`.
It explains how ledger time is made available across contracts and what each contract should treat as the authoritative current time.

---

## Model Summary

In Soroban, ledger time is not passed explicitly from one contract to another.
Instead, every contract reads the same host-provided clock via:

```rust
let now: u64 = env.ledger().timestamp();
```

That value is the canonical current time for the current transaction and the
contract's execution context.

### Core rule

- `env.ledger().timestamp()` is the authoritative clock.
- Every contract must read time from the host, not from untrusted caller-supplied timestamps.
- Within a single transaction, all contracts see the same ledger timestamp.

---

## What this means for cross-contract flows

### 1. Time is implicit, not passed as metadata

A contract does not receive a special “transaction time” value from the caller.
It obtains time directly from the Soroban host using `env.ledger().timestamp()`.
That is true for every contract involved in cross-contract execution.

### 2. Same transaction = same time

If contract `A` calls contract `B` within the same transaction, both observe the
same `env.ledger().timestamp()` value.
This makes it safe for nested cross-contract calls to compare deadlines,
expiry timestamps, and schedule due dates against the current ledger time.

### 3. Historical timestamps are just stored values

If a contract captures the current time and later passes it as an argument,
the callee must treat that value as a historical timestamp, not as the current time.
The only authoritative current time is still `env.ledger().timestamp()`.

```rust
let created_at = env.ledger().timestamp();
// store created_at
// later, another contract may receive created_at as an argument,
// but the current time remains env.ledger().timestamp().
```

---

## Concrete examples

### Example: orchestrator deadline validation

This is the actual contract pattern used in `orchestrator/src/lib.rs`.
The deadline is validated against the current ledger time before any downstream
cross-contract calls are executed.

```rust
let now = env.ledger().timestamp();
if deadline <= now {
    return Err(OrchestratorError::DeadlineExpired);
}
```

Because the orchestrator and its downstream contracts execute in the same
transaction, any downstream call that also reads `env.ledger().timestamp()` sees
this same `now` value.

### Example: deadline semantics

Use `<=` when a stored deadline is no longer valid.

```rust
if now >= target_due_date {
    // deadline has passed or is exactly due
}
```

That same comparison style is used across contracts such as `bill_payments`,
`savings_goals`, and `insurance`.

### Example: age and elapsed time

Compute elapsed time with `saturating_sub` to avoid underflow if a stored
timestamp is unexpectedly in the future.

```rust
let age = env.ledger().timestamp().saturating_sub(snapshot_taken_at);
```

This is the same style used by shared helpers in `remitwise-common`.

---

## Practical guidance for contributors

- Always call `env.ledger().timestamp()` in the contract that makes the decision.
- Do not rely on caller-provided timestamp arguments as the current ledger time.
- When comparing time values:
  - use `>=` for expiry checks
  - use `<=` for future-only guards
  - use `saturating_sub` for age/difference calculations
- In cross-contract flows, trust the host clock rather than trying to forward
  a timestamp manually.

---

## Why this matters

This contract model avoids accidental time drift or replay confusion between
contracts.
Because the host provides a single, shared ledger timestamp for the current
transaction, every contract in the flow observes the same current time.

If a contract instead treated an argument as the current time, it could be fooled
by stale or malicious values from another contract or caller.

---

## See also

- [`docs/TIMESTAMP_CONVENTIONS.md`](TIMESTAMP_CONVENTIONS.md) — general timestamp rules across the workspace
- [`docs/CROSS_CONTRACT_EPOCHS.md`](CROSS_CONTRACT_EPOCHS.md) — how cross-contract epochs prevent stale replay
- [`docs/CROSS_CONTRACT_INVARIANTS.md`](CROSS_CONTRACT_INVARIANTS.md) — invariants spanning multiple contracts
