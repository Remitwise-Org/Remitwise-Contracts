# Canonical remittance split request hashes

The public `get_request_hash(DistributeUsdcRequest)` helper is the only
supported source of signing preimages. It binds the distribution domain,
current contract address, token contract, caller, all four destinations,
amount, nonce, and deadline in a fixed order before computing SHA-256.

The generated client tests in `remittance_split/tests/request_hash_vectors.rs`
exercise every field independently, amount sign and zero boundaries, nonce and
deadline boundaries, recipient swaps, cross-contract use, and repeated vector
generation. A signer must not reconstruct the payload by concatenating fields
itself because a missing field changes the replay domain.

## Compatibility

The helper returns 32 bytes and preserves the existing contract entrypoint and
XDR shape. The vector suite is additive and does not alter storage. A future
hash algorithm or field-order change requires a new domain/version and a new
helper; silently changing the current preimage would invalidate signatures in
flight and make rollback ambiguous.

## Verification checklist

- [x] operation domain is fixed by `DISTRIBUTE_USDC_DOMAIN` and `distrib`
- [x] contract and token addresses are included
- [x] caller and every destination account are included
- [x] signed amount, nonce, and deadline are included
- [x] duplicate calls are deterministic
- [x] cross-contract and field-substitution cases have regression tests
- [x] no raw secrets or signature material are stored

## Failure-mode guidance

Callers should calculate the hash immediately before signing and pass the
same typed request to the contract. A request hash is not a substitute for
authorization: the contract still authenticates the caller and validates the
nonce and deadline. Hash equality proves only that the signed fields have not
been substituted.

The tests intentionally include invalid business values because encoding and
validation are separate concerns. A zero amount can be encoded deterministically
even though distribution must reject it. This prevents a future implementation
from accidentally making validation behavior part of the hash format.

When adding a field, update the typed request, the helper's documented order,
the known-good vectors, and every mutation test. Do not append an optional
field without a versioned domain separator: old signatures must not become
ambiguous with new request layouts.

The vector suite is deterministic and bounded, so it is suitable for CI and
for reproducible audits across native and WASM builds.
Audit failures should preserve the stable error taxonomy for integrators.
