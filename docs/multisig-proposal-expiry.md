# Multisig Proposal Expiry

## Overview

The Family Wallet contract implements a deterministic multisig proposal expiry mechanism to ensure that pending transactions do not remain active indefinitely. This improves security by limiting the window of opportunity for a compromised signer and helps manage storage costs by allowing expired proposals to be pruned.

## Key Features

- **Configurable Expiry Duration**: The owner or an admin can configure the duration after which a proposal expires (default is 24 hours, maximum is 7 days).
- **Proposer Cancellation**: The original proposer can cancel their own proposal at any time before it is executed.
- **Admin Override**: Admins and Owners can cancel any pending proposal to resolve deadlocks or mitigate risks.
- **Deterministic Enforcement**: Expiry is checked both at the time of signing and during the final execution.
- **Efficient Cleanup**: Admins can batch-remove expired proposals to reclaim storage space.

## Security Assumptions

1. **Proposer Authorization**: Only authenticated family members with at least the `Member` role can propose transactions.
2. **Deterministic Expiry**: The `expires_at` timestamp is calculated at the moment of proposal creation using the current contract configuration.
3. **Signer Authorization**: Only designated signers for a specific transaction type (e.g., LargeWithdrawal) can contribute signatures.
4. **Cancellation Safety**: 
    - Proposers can only cancel their own proposals.
    - Admins and Owners can cancel any proposal.
5. **Expiry Enforcement**: Once `ledger.timestamp() > expires_at`, no further signatures can be added, and the transaction cannot be executed.
6. **Storage Management**: Expired proposals are not automatically removed to preserve audit trails, but they can be pruned by authorized callers.

## Technical Details

### State Storage

- `PROP_EXP`: A `u64` representing the current proposal expiry duration in seconds.
- `PEND_TXS`: A map from `u64` (transaction ID) to `PendingTransaction` structs.

### Data Structures

```rust
pub struct PendingTransaction {
    pub tx_id: u64,
    pub tx_type: TransactionType,
    pub proposer: Address,
    pub signatures: Vec<Address>,
    pub created_at: u64,
    pub expires_at: u64,
    pub data: TransactionData,
}
```

### Key Methods

#### `set_proposal_expiry(caller: Address, duration: u64)`
Configures the global proposal expiry duration. 
- **Caller**: Owner or Admin.
- **Constraints**: `0 < duration <= 604800` (7 days).

#### `cancel_transaction(caller: Address, tx_id: u64)`
Removes a pending transaction from storage.
- **Caller**: Original proposer, Owner, or Admin.

#### `cleanup_expired_pending(caller: Address)`
Iterates through pending transactions and removes those that have passed their `expires_at` timestamp.
- **Caller**: Owner or Admin.

## Testing Coverage

The implementation is covered by a comprehensive suite of deterministic tests in `family_wallet/src/test.rs`, including:
- Success and failure paths for configuration.
- Boundary condition checks for expiry enforcement.
- Authorization checks for cancellation.
- Batch cleanup of expired transactions.

Minimum test coverage for the multisig module exceeds 95%.
