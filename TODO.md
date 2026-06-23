- [ ] Add quorum unreachable -> revalidate_proposals invalidation tests (events, return count, untouched-reachable, idempotency, auth) to family_wallet/src/test.rs
- [ ] Add edge-case tests: threshold vs signer count; signer already signed; no pending proposals returns 0
- [ ] Add docs note describing revalidate_proposals semantics + event emission + idempotency
- [x] Add quorum unreachable -> revalidate_proposals invalidation tests (events, return count, untouched-reachable, idempotency, auth) to family_wallet/src/test.rs
- [x] Add edge-case tests: threshold vs signer count; signer already signed; no pending proposals returns 0
- [x] Add docs note describing revalidate_proposals semantics + event emission + idempotency
- [ ] Run: cargo test -p family_wallet revalidate_proposals -- --nocapture
- [ ] Run: cargo test -p family_wallet
- [ ] Run: cargo clippy -p family_wallet (ensure clean)


