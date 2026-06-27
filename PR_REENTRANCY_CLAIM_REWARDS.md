# Add reentrancy guard to `claim_rewards_summary_external`

Closes #2 (security-002-reentrancy-protection)

---

## Threat mitigated — T-RE-02: re-entrant reward drain

**What an attacker gains if this check is missing:**

Without the lock, a malicious (or compromised) reward-token contract can
exploit the classic read-then-call-then-write reentrancy pattern:

1. Attacker calls `claim_rewards_summary_external` for an address with
   `pending = 1 000` tokens.
2. The orchestrator reads `pending = 1 000` from storage.
3. The orchestrator calls `token.transfer(this, attacker, 1_000)`.
4. The malicious token contract re-enters `claim_rewards_summary_external`
   before the orchestrator has written `pending = 0`.
5. The re-entrant call reads `pending = 1 000` again and triggers a second
   transfer.
6. Steps 3–5 repeat for as long as the attacker's token contract cooperates,
   draining funds beyond entitlement.

---

## Changes

### `orchestrator/src/lib.rs`

| What | Why |
|---|---|
| Added `RewardTokenInterface` client trait (`transfer`) | Typed client for the external SEP-41 token contract |
| Added `PENDING_REWARDS` storage key (`Map<Address, i128>`) | Tracks per-address accrued reward balances |
| Added `OrchestratorError::ReentrancyDetected = 12` | Typed error instead of panic — observable by indexers |
| Added `OrchestratorError::NoPendingRewards = 13` | Guard against zero-balance claims |
| Added `claim_rewards_summary_external(caller, reward_token)` | The guarded entry-point |
| Added `credit_pending_rewards` (private helper) | Accrual helper for future flow integration |
| Added `get_pending_rewards(address)` (public read-only) | Off-chain balance inspection |

**Security properties of `claim_rewards_summary_external`:**

1. **Reentrancy guard** — `EXEC_LOCK` is acquired (via RAII `LockGuard`) before
   any storage read. A re-entrant call while the lock is held returns
   `ReentrancyDetected` immediately with no state mutation.
2. **Checks-Effects-Interactions** — the pending balance is **zeroed in storage
   before** the external `token.transfer` call. Even if the lock check
   somehow passed in a re-entrant call, the second call would see `pending = 0`
   and return `NoPendingRewards`.
3. **Typed errors** — neither guard panics; both surfaces `#[contracterror]`
   variants visible to off-chain tooling.
4. **Authorization first** — `caller.require_auth()` before any storage access.
5. **Lock release on all paths** — `LockGuard` RAII ensures `EXEC_LOCK` is
   reset to `false` on normal return, early `?` return, and Soroban-level
   panic (state rollback).

### `orchestrator/src/test.rs`

Four new tests covering the new entry-point:

| Test | Purpose |
|---|---|
| `test_claim_rewards_summary_external_happy_path` | Normal claim succeeds, returns amount, zeroes balance |
| `test_claim_rewards_summary_external_no_pending_returns_error` | Zero-balance caller gets `NoPendingRewards` |
| **`test_claim_rewards_summary_external_rejects_reentrant_call`** | **NEGATIVE TEST** — lock held → `ReentrancyDetected`, balance unchanged |
| `test_claim_rewards_summary_external_cei_order_balance_zeroed_before_transfer` | CEI: storage zeroed before external call |

The negative test (`test_claim_rewards_summary_external_rejects_reentrant_call`)
**would fail without the guard** (the function would succeed and drain the
balance) and **passes after the fix**.

### Pre-existing fixes (required for compilation)

- `remitwise-common/Cargo.toml` — removed two duplicate `[features]` sections
  that caused `cargo` to fail with `error: duplicate key`.
- `remitwise-common/src/lib.rs` — fixed `verify_signature` which used
  `Vec::with_capacity` (std) and a non-existent `soroban_sdk::crypto::ed25519_verify`
  API. Replaced with `env.crypto().ed25519_verify` (soroban 21.x) and
  `soroban_sdk::Bytes` for the `#![no_std]` message buffer.
- `orchestrator/src/test.rs` / `events_schema_test.rs` — suppressed
  `clippy::duplicated_attributes` on the inner `#![cfg(test)]` (lint triggered
  because the files are already gated via `#[cfg(test)] mod test` in lib.rs).

---

## Test results

```
running 69 tests
... 68 passed; 1 failed (test_wasm_artifacts_respect_documented_size_budgets —
    requires pre-built WASM artifacts, unrelated to this change)
```

`cargo clippy -p orchestrator --all-targets -- -D warnings` exits 0.

---

## Cost estimate

`claim_rewards_summary_external` adds two instance-storage reads (lock check +
pending rewards map), one instance-storage write (zero out balance + lock set),
one cross-contract call, and one lock-release write on drop — identical to the
existing `execute_remittance_flow` hot path. No measurable regression on normal
(non-re-entrant) calls.
