# Remittance Split — Self-Transfer Guard

## Overview

A *self-transfer* occurs when the sender address (`from`) is also listed as one of the
split destination accounts (`accounts.spending`, `accounts.savings`, `accounts.bills`,
or `accounts.insurance`). Such a call is a no-op from a fund-movement perspective — the
token contract would simply move tokens from an address back to itself — but it would
still advance the replay-protection nonce, pollute the audit log with a spurious success
entry, and waste gas.

To prevent this, the contract enforces a **self-transfer guard** that rejects any call
where `from` matches any destination before any state-changing side-effects are applied.

---

## Affected Functions

| Function | Path |
|---|---|
| `distribute_usdc` | `remittance_split/src/lib.rs` |
| `distribute_usdc_signed` | `remittance_split/src/lib.rs` |

---

## Check Ordering Contract

Both functions enforce the following execution order:

1. **Self-transfer guard** — returns `SelfTransferNotAllowed` if `from == destination`;
   records an audit failure entry; nonce is untouched.
2. **Nonce retrieval** via `get_nonce()`.
3. **[signed path only] Signature / hash verification.**
4. **Nonce mutation** via `symbol_short!("NONCES")`.
5. **Token transfer execution.**
6. **Audit success entry** via `append_audit(..., true)`.
7. **`DistributionCompletedEvent` emission.**

The guard is placed at step 1 to ensure that **no NONCES storage read or write ever
occurs for a self-transfer call**. This is the primary security invariant enforced by
the ordering contract.

---

## Security Rationale

### Why the guard MUST precede nonce mutation

The nonce is a replay-protection counter: each successful operation increments it so
that a previously-observed signed message cannot be reused. If the guard were placed
*after* nonce mutation, a self-transfer attempt would advance the counter even though
no legitimate transfer occurred. An attacker could exploit this to:

1. Desynchronise the on-chain nonce from a client's expected sequence, causing future
   legitimate calls to fail with `InvalidNonce`.
2. Burn a valid nonce value without performing a real transfer, effectively denying
   service to the owner.

By rejecting before any NONCES read or write, the contract ensures that **a rejected
self-transfer has zero replay-protection side-effects**.

### Why the guard MUST precede nonce *read* as well

`require_nonce` (and `require_nonce_hardened`) read the NONCES map. Even a read-only
access would be wasteful here: we know the call is invalid the moment we inspect the
accounts. Placing the guard before any NONCES access also keeps the rejection path
free of unnecessary storage I/O, reducing gas cost on the failure path.

---

## Audit Log Behaviour

On every self-transfer rejection the contract calls:

```rust
Self::append_audit(&env, symbol_short!("distrib" | "distH"), &from, false);
```

This means:

- A **failure entry** (`success = false`) is always appended to the on-chain audit log.
- The entry records the caller address and the ledger timestamp.
- The log is queryable via `get_audit_log(from_index, limit)`.
- No events are published on the rejection path — events are only emitted on success.

### Unit-test limitation

In the Soroban test environment (soroban-sdk 21.x), returning `Err(...)` from a
contract function causes **all storage mutations within that invocation to be reverted**.
This means the `append_audit(..., false)` call inside the guard is rolled back and is
not visible via `get_audit_log()` in unit tests.

The unit tests therefore verify that the audit log is **unchanged** after a rejection
(confirming no spurious success entry was added), and rely on on-chain integration
tests to confirm the failure entry persists in production where `Err` is a committed
contract error, not a state-reverting trap.

---

## Error Reference

| Variant | Discriminant | Condition |
|---|---|---|
| `SelfTransferNotAllowed` | **13** | Any destination account equals the sender (`from`) |

The variant discriminant is stable across contract upgrades. External consumers
(indexers, SDKs) may identify this error either by name or by the value `13`.

---

## Test Coverage

| Test | Function | What It Proves |
|---|---|---|
| **A** — `test_a_distribute_usdc_basic_self_transfer_rejection` | `distribute_usdc` | Guard fires, error is `SelfTransferNotAllowed`, nonce unchanged, no events, audit log unchanged (storage revert in test env) |
| **B** — `test_b_distribute_usdc_signed_valid_sig_self_dest` | `distribute_usdc_signed` | Guard fires even with a valid request hash; nonce unchanged, no token movement, no events, audit log unchanged |
| **C** — `test_c_distribute_usdc_signed_nonce_invariant_after_rejection` | `distribute_usdc_signed` | `nonce_before == nonce_after` strict equality after rejection |
| **D** — `test_d_distribute_usdc_non_self_transfer_succeeds` | `distribute_usdc` | Positive case: distinct destination passes guard, nonce incremented by 1, audit success |
| **E** — `test_e_distribute_usdc_all_destinations_self` | `distribute_usdc` | Full self-split (all four accounts == from) is rejected; nonce unchanged |

---

## Worked Example

### Scenario

Alice (`ADDR_ALICE`) tries to call `distribute_usdc` with her own address as the
spending destination:

```
usdc_contract = ADDR_USDC
from          = ADDR_ALICE
nonce         = 3
deadline      = now + 600
accounts      = {
    spending  = ADDR_ALICE,   ← same as from!
    savings   = ADDR_SAVINGS,
    bills     = ADDR_BILLS,
    insurance = ADDR_INSURANCE
}
total_amount  = 1_000_000
```

### Execution Trace

```
distribute_usdc(...)
  │
  ├─ 1. from.require_auth()                  → OK  (Alice authorises)
  ├─ 2. require_not_paused()                 → OK  (contract is live)
  ├─ 3. config = storage.get("CONFIG")       → OK  (contract initialised)
  ├─ 4. config.owner == from?                → OK  (Alice is the owner)
  ├─ 5. config.usdc_contract == usdc_contract → OK  (trusted token)
  ├─ 6. total_amount > 0?                    → OK  (1_000_000 > 0)
  │
  ├─ 7. Self-transfer guard                  ← TRIGGERED
  │      accounts.spending == from           → TRUE
  │      append_audit("distrib", ADDR_ALICE, false)
  │      return Err(SelfTransferNotAllowed)  ← FUNCTION RETURNS HERE
  │
  │   ── steps below are never reached ──
  ├─ 8. require_nonce_hardened(...)          ✗ skipped — NONCES not touched
  ├─ 9. token.transfer(...)                  ✗ skipped — no funds moved
  ├─ 10. increment_nonce(...)                ✗ skipped — nonce stays at 3
  ├─ 11. append_audit(..., true)             ✗ skipped
  └─ 12. DistributionCompletedEvent          ✗ not emitted
```

### Outcome

- **Error returned:** `RemittanceSplitError::SelfTransferNotAllowed` (variant 13)
- **Nonce:** still `3` — unchanged
- **Token balances:** unchanged — no transfer executed
- **Audit log:** one new entry with `success = false`
- **Events:** no new events published
