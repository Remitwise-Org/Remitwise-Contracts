FW Signature Deduplication

This note documents the multisig signature idempotency guarantees implemented in `family_wallet`.

Summary

- Repeated calls to `sign_transaction` by the same `signer` for the same `tx_id` are treated as a no-op and do not increase the recorded approval count.
- The proposer is implicitly recorded as an initial approver when a proposal is created; subsequent calls by the proposer will not double-count that approval.
- Non-authorized signers (addresses not in the configured `signers` vector for the transaction type) are rejected with `SignerNotMember`.
- Expired proposals return `TransactionExpired` when signing is attempted.

Security rationale

Deduplication prevents a single account from repeatedly calling `sign_transaction` to artificially inflate the approval count and meet the `threshold` without needing distinct signers. This preserves the intended multisig guarantee and prevents a single compromised signer from unilaterally executing a multisig action via repetition.

Developer notes

- `sign_transaction` now returns `Result<bool, Error>`; callers who want to observe the exact error should use the generated `try_sign_transaction` client wrapper in tests or off-chain tooling.
- Successful first-time signatures (including those that cause execution) return `Ok(true)`. Repeated signatures return `Ok(false)` to indicate a no-op. Errors return the appropriate `Error` variant.

Testing

- Unit tests added in `family_wallet/src/test.rs` cover duplicate-sign idempotency and non-member rejection.


