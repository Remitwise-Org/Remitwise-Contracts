# Security finding: stale signer signatures can count after signer rotation

## Summary

While adding the requested signer-rotation coverage, the current `family_wallet` implementation appears to contain the exact quorum-integrity issue described by the issue.

`sign_transaction` checks quorum using the raw length of `PendingTransaction.signatures`:

```rust
if pending_tx.signatures.len() >= config.threshold {
    // execute transaction
}
```

Because the stored signature vector is not re-filtered against the latest `MultiSigConfig.signers`, a signer who was removed by `configure_multisig` can still have an old signature counted toward the new quorum.

## Impact

A compromised signer can be removed from the multisig signer set, but any approval they already placed on an in-flight proposal may still help execute the transaction after rotation. This is most dangerous during signer rotation, which is precisely when compromised keys are expected to be removed.

## Reproduction scenario

1. Configure `LargeWithdrawal` as 3-of-3: `[owner, signer_a, signer_b]`.
2. Owner proposes a withdrawal and auto-signs.
3. `signer_a` signs, leaving two signatures: `[owner, signer_a]`.
4. Rotate signers to `[owner, signer_b, signer_c]`, removing `signer_a` and adding `signer_c`.
5. `signer_c` signs.
6. Current implementation can execute because `signatures.len() == 3`, even though only `[owner, signer_c]` are valid under the new signer set.

## Added regression coverage

`family_wallet/tests/signer_rotation.rs` adds tests for:

- stale rotated-out signatures not counting toward quorum;
- newly rotated-in signers signing and reaching quorum;
- impossible threshold rotations being rejected;
- removing the proposer and ensuring the auto-signature does not count after rotation.

## Recommended fix direction

Before checking quorum, count only signatures that are members of the current `MultiSigConfig.signers`. On `configure_multisig`, call a `revalidate_proposals` routine that either:

1. removes stale signatures from affected pending transactions, or
2. invalidates affected transactions and emits `ProposalInvalidatedEvent` with a membership-change reason.

A production fix should also add an explicit invalidation event type so indexers and wallets can explain why a proposal disappeared or became unexecutable.

## Review note

This branch intentionally documents the finding alongside tests because the issue instructions say to stop and document the security finding if a genuine quorum-bypass is found. The production logic fix should be reviewed separately before merge.
