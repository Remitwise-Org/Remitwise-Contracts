# SC-001 Remittance Split: Request-Hash Helpers for distribute_usdc

## Status: DONE ✅

### Implementation Complete

- [x] `DistributeUsdcRequest` struct with all signed fields
- [x] `get_request_hash()` public helper returning SHA-256 `Bytes`
- [x] `distribute_usdc_hashed()` entrypoint with hash verification
- [x] `require_nonce_hardened()` multi-layer replay protection
- [x] Domain separation via `DISTRIBUTE_USDC_DOMAIN` constant
- [x] Parameter binding: all 9 fields included in hash preimage

### Test Coverage (95%+)

- [x] Hash determinism (same input → same hash)
- [x] Parameter sensitivity (any field change → different hash)
- [x] Deadline boundary tests (zero, past, now, max, beyond max)
- [x] Hash mismatch detection (wrong hash rejected)
- [x] Cross-call consistency
- [x] Self-transfer rejection
- [x] Untrusted token rejection
- [x] Nonce reuse / replay protection
- [x] Account reordering protection
- [x] Domain-id swap protection
- [x] Expired deadline does not advance nonce

### Documentation

- [x] `REQUEST_HASH_SIGNER_GUIDE.md` - integrator and signer workflow
- [x] `SECURITY_TESTS_DEADLINE_HASH.md` - threat model & test matrix
- [x] `docs/remittance-split-request-hash.md` - preimage spec
- [x] Signer workflow with parameter binding table
- [x] FAQ covering common questions
- [x] Troubleshooting guide for hash/nonce/deadline issues
