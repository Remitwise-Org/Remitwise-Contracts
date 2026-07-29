# Expiry Guard — `sign_transaction` / `proposal_expired`

## Overview

The expiry guard is a runtime check in `sign_transaction` that prevents expired multisig proposals from being signed or executed. It is independent of the `cleanup_expired_pending` maintenance function — even if cleanup has never been called, an expired proposal cannot produce a valid signature or trigger execution.

## Design

### Helper: `proposal_expired`

Defined in `family_wallet/src/lib.rs` as a private helper on `FamilyWallet`:

```rust
fn proposal_expired(env: &Env, pending: &PendingTransaction) -> bool
```

Returns `true` when `env.ledger().timestamp() > pending.expires_at` — i.e., the ledger time is strictly past the proposal's expiry boundary. The strict `>` means a proposal with `timestamp == expires_at` is still valid.

### Integration with `sign_transaction`

At `family_wallet/src/lib.rs`, `sign_transaction` calls `Self::proposal_expired(&env, &pending_tx)` immediately after loading the pending transaction and before any of the following:

- Duplicate-signature detection
- Signer-authorization checks (multi-sig config)
- Signature accumulation
- Threshold-based execution

This ordering ensures the guard fires as early as possible, rejecting the entire operation before any state mutation.

### Expiry disabled (`PROP_EXP = 0`)

When the global `PROP_EXP` is set to `0` (via `set_proposal_expiry`), proposals are created with `expires_at = u64::MAX`. Because no realistic ledger timestamp can exceed `u64::MAX`, the guard always passes — this effectively disables the expiry mechanism.

## Cross-references

- **`PROP_EXP`**: Global `u64` stored under `symbol_short!("PROP_EXP")`; configured by `set_proposal_expiry`; read by `get_proposal_expiry_public` and `propose_transaction`.
- **`PendingTransaction.expires_at`**: Per-proposal field set at creation time to `created_at + PROP_EXP` (or `u64::MAX` when disabled).
- **`cleanup_expired_pending`**: Maintenance function that prunes expired proposals from `PEND_TXS`. Independent of the expiry guard — an expired proposal is rejected at sign/execute time regardless of whether it has been cleaned up.
- **Existing documentation**: See [`docs/multisig-proposal-expiry.md`](multisig-proposal-expiry.md) for the broader proposal expiry design.

## Test Coverage

| Test | What it verifies |
|------|------------------|
| `test_proposal_expiry_default_enforced` | Default 24h expiry, sign after `expires_at + 1` is rejected |
| `test_proposal_expiry_exact_boundary` | Sign at exactly `expires_at` still succeeds (strict `>` semantics) |
| `test_expiry_disabled_zero` | `PROP_EXP = 0` creates proposals that never expire |
| `test_sign_past_expiry_execute_rejected` | Threshold-reaching sign past expiry is rejected (execute path) |

## Security Notes

- The guard runs *before* signature accumulation, preventing storage writes for expired proposals.
- There is no race between cleanup and sign — the guard checks ledger time directly.
- The `cleanup_expired_pending` function uses `expires_at < current_time` (strict less) which matches the guard's `current_time > expires_at`.
