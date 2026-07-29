# Family Wallet: Batch Membership Semantics

This document defines the batch behavior for `family_wallet::batch_add_family_members` and `family_wallet::batch_remove_family_members`.

## Contracted Semantics

Both batch entry points are all-or-nothing.

- If every item in the batch is valid, the contract applies every change and returns the number of items processed.
- If any item is invalid, the whole call aborts and no membership state is changed.
- Empty batches are valid no-ops and return `0`.

## Validation Rules

### `batch_add_family_members`

- Maximum batch length: `MAX_BATCH_MEMBERS` (`30`).
- Maximum total family members after the batch: `MAX_FAMILY_MEMBERS` (`30`, including the owner).
- Every address in the batch must be unique.
- No batch item may target the owner role.
- No address may already exist in `MEMBERS`.

### `batch_remove_family_members`

- Maximum batch length: `MAX_BATCH_MEMBERS` (`30`).
- Every address in the batch must be unique.
- The owner cannot be removed.
- Every address must already exist in `MEMBERS`.

## Failure Mode

The contract rejects invalid batches by panicking before any storage mutation is committed. Mixed-validity batches therefore never produce partial membership state.

## Return Values

- `batch_add_family_members` returns the number of members added on success.
- `batch_remove_family_members` returns the number of members removed on success.
- Empty batches return `0`.

## Security Notes

- Pre-validation prevents intra-batch duplicates from overwriting a later item.
- The member cap is enforced against the post-batch total, not just the batch length.
- Because batch writes are atomic, a failed member-add/remove request cannot leave `MEMBERS` in a partially updated state.