# Remittance nonce replay policy

Every signed remittance operation uses a signer-scoped sequential nonce. The
current value is read from instance storage and the request must match it
exactly. A successful operation records the consumed nonce and advances the
counter; stale, future, duplicate, expired, and hash-mismatched requests return
typed errors before any token transfer or nonce mutation.

Deadlines are strict: a deadline at or before the ledger timestamp is stale,
and a deadline outside the bounded future window is rejected. The request hash
binds the operation domain, contract, signer, token, four destinations, amount,
nonce, and deadline. This prevents a valid signature from being moved to a
different contract, operation, recipient, or settlement time.

The matrix in `nonce_replay_matrix.rs` checks successful monotonic advancement,
duplicate submissions, stale/future values, invalid amount and hash behavior,
recipient isolation, separate signers, separate contract instances, and the
absence of token-side effects after rejection. Failed attempts must not skip a
nonce, so a valid request can still be retried with the same current revision.

Nonce storage is intentionally not a global counter. The signer and contract
instance are part of the authorization domain, while the typed request hash
binds the operation-level fields. Snapshot/import code must preserve or
explicitly re-establish this invariant before accepting signed work.

Overflow is fail-closed: advancing `u64::MAX` returns the stable `Overflow`
error rather than wrapping to zero. Operators should treat that state as a
rotation/migration event, not reset the counter in place.

## Reviewer checklist

- [x] authentication remains required by the contract entrypoint
- [x] stale and future deadlines are rejected before mutation
- [x] sequential nonce mismatch is rejected before token calls
- [x] used nonce is rejected on replay
- [x] request hash mismatch is rejected before transfer
- [x] rejected calls leave nonce and recipient balances unchanged
- [x] signer and contract domains remain isolated

## Integration guidance

A client should first read the current nonce, construct one typed request, ask
the contract for its request hash, and sign that exact byte sequence. The
client must not reuse a hash after changing a field. If a submission fails
because the nonce is stale, it must fetch the new nonce and construct a new
request rather than guessing the next value.

The contract's error variants are part of the integration surface. `InvalidNonce`
means the sequential counter did not match; `NonceAlreadyUsed` identifies a
replay that reached the used-set check; `DeadlineExpired` identifies a stale
or too-distant window; and `RequestHashMismatch` identifies field substitution.
Callers should log the code and request correlation ID, never the raw signed
payload or private signing material.

The deadline policy uses the ledger clock, not a client wall clock. A client
may choose a small future window to reduce replay exposure, but it must leave
enough time for transaction propagation. Boundary tests intentionally assert
that equality with the current timestamp is rejected.

The used-nonce set is bounded to prevent unbounded instance storage growth.
The sequential counter remains authoritative for the normal path, while the
used set provides defense in depth for snapshot restoration and replay checks.
Any migration that changes retention or snapshot behavior must preserve the
signer/operation domain and document how old signed requests are invalidated.

No concurrent request may observe a partially advanced nonce: Soroban invokes
the state change atomically. A failed contract invocation rolls back both the
transfer calls and nonce writes, which is why the tests inspect balances and
the nonce after every rejection scenario.

For audit reviews, compare the public request type, the hash helper's field
order, the entrypoint validation order, and the generated client signatures.
All four must describe the same domain. A mismatch between any pair can turn
an otherwise valid signature into a confused-deputy or replay vulnerability.
