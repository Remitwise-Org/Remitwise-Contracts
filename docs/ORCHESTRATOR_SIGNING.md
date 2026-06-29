# Orchestrator Signed-Flow Request-Hash and Deadline Model

This document describes the signed-flow execution path in the Remitwise orchestrator contract — specifically how the request hash is constructed and how the deadline is validated to prevent replay attacks and stale execution.

## Overview

The `execute_remittance_flow_signed` entrypoint accepts a caller-supplied hash and deadline alongside the standard flow parameters. This allows off-chain actors (mobile apps, backend services) to pre-authorize a remittance flow at signing time and submit it to the chain within a bounded window.

## Request Hash Construction

The request hash is a 64-bit value computed by the orchestrator in `compute_request_hash`:

```
hash = (op_bits + nonce + amount_lo + amount_hi + deadline) × 1_000_000_007 (mod 2^64)
```

Where:
- `op_bits` — the `Symbol` for the operation (e.g. `"exec_flow"`) converted to its raw `Val` payload.
- `nonce` — the current per-address nonce (monotonically increasing, burns on use).
- `amount_lo` — lower 64 bits of the signed `i128` amount.
- `amount_hi` — upper 64 bits of the signed `i128` amount.
- `deadline` — the caller-supplied Unix timestamp (seconds since epoch) by which the request must be processed.

All fields are mixed with wrapping addition. The final multiply by the prime `1_000_000_007` acts as a cheap avalanche step that distributes bit-flips across the full 64-bit word.

### Security Properties

| Property | Mechanism |
|---|---|
| Cross-operation binding | `op_bits` encodes the operation symbol |
| Per-caller uniqueness | `nonce` is per-address, incremented atomically on use |
| Amount binding | Both halves of `i128` included — no truncation |
| Expiry binding | `deadline` included in hash — changing deadline invalidates hash |
| Replay prevention | Nonces are recorded in a bounded ring-buffer and rejected on re-use |

> **Note**: This is not a cryptographic MAC. It provides collision resistance sufficient to prevent accidental reuse, not adversarial forgery. For a production signing model, callers should use an off-chain Ed25519 signature over the same fields and the orchestrator should verify it with `remitwise_common::verify_signature`.

## Deadline Model

The `deadline` field is a Unix timestamp (seconds). The orchestrator enforces:

```
current_ledger_timestamp <= deadline <= current_ledger_timestamp + MAX_DEADLINE_WINDOW_SECS
```

Where `MAX_DEADLINE_WINDOW_SECS = 3600` (1 hour).

### Deadline Validation Steps

1. **Expiry check**: `deadline < current_time` → `DeadlineExpired`
2. **Window check**: `deadline > current_time + MAX_DEADLINE_WINDOW_SECS` → `DeadlineExpired`
3. **Hash verification**: computed hash must equal the caller-supplied `request_hash` → `InvalidNonce`
4. **Nonce check**: the nonce must not already appear in the replay-protection ring-buffer → `NonceAlreadyUsed`
5. **Execution**: `execute_flow_internal` runs under `EXEC_LOCK`
6. **Nonce burn**: nonce recorded in ring-buffer, caller nonce incremented

### Replay Protection Ring-Buffer

Each address maintains up to `MAX_USED_NONCES_PER_ADDR = 256` used nonces. When the buffer is full, the oldest entry is evicted (sliding window). This bounds instance-storage rent while preventing replay within the window.

## Recalculation Triggers

A new request hash must be computed whenever any of these change:

- The operation symbol (different operation type)
- The caller's current nonce (e.g. after a previous signed call consumed it)
- The `amount` parameter
- The `deadline` (even by one second, since it is included in the hash)

## Example (Off-Chain Signing Flow)

```
1. Fetch current nonce: get_nonce(caller_address)
2. Choose deadline:      now + 600  (10 minutes)
3. Compute hash:         compute_request_hash("exec_flow", nonce, amount, deadline)
4. Submit tx:            execute_remittance_flow_signed(caller, amount, nonce, deadline, hash, params)
5. On success:           nonce is incremented on-chain — repeat from step 1 for next call
```

## Error Reference

| Error | Cause |
|---|---|
| `DeadlineExpired` | `deadline < now` or `deadline > now + 3600` |
| `InvalidNonce` | computed hash ≠ caller-supplied hash |
| `NonceAlreadyUsed` | nonce already in replay-protection ring-buffer |
| `ExecutionLocked` | reentrancy: `EXEC_LOCK` is held by an outer call |
