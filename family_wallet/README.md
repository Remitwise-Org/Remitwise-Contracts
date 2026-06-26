# Family Wallet Contract

A Soroban smart contract for multi-sig family wallet management with role-based permissions, spending limits, and emergency controls.

## Features

- Multi-member family wallets with roles (Owner, Admin, Member)
- Multi-sig transactions with configurable thresholds per transaction type
- Per-member spending limits with precision controls and rollover
- Emergency transfer mode with cooldowns and minimum balance checks
- Pause/unpause functionality
- Audit logging with pagination
- Archived transaction history
- Global and per-function pause controls

## Quickstart

```rust
use family_wallet::{FamilyWalletClient, FamilyRole, TransactionType};

// 1. Initialize the wallet
client.init(&owner, &vec![&env, alice.clone()]);

// 2. Add a member
client.add_member(&owner, &bob, &FamilyRole::Member, &1_000_0000000);

// 3. Propose a transaction
let tx_id = client.propose_withdrawal(
    &bob,
    &token_addr,
    &bob,
    &1_000_0000000,
);

// 4. Sign and execute
client.sign_transaction(&alice, &tx_id);
```

## API Reference

### Initialization

#### `init(env, owner, initial_members)`

Initializes the family wallet contract. Must be called first.

### Member Management

#### `add_member(env, caller, new_member, role, spending_limit)`

Adds a new member with a role and spending limit. Owner/Admin only.

#### `remove_member(env, caller, member_to_remove)`

Removes a member from the wallet. Owner/Admin only.

#### `get_members(env, owner, from_index, limit)`

Returns paginated list of members.

### Spending Limits

#### `set_spending_limit(env, caller, member, new_limit)`

Updates a member's spending limit. Owner/Admin only.

### Multi-sig Transactions

#### `propose_transaction(env, caller, tx_type, data, expiry)`

Proposes a new transaction for sign-off.

#### `sign_transaction(env, caller, tx_id)`

Signs a pending transaction.

#### `execute_transaction(env, caller, tx_id)`

Executes a transaction that has reached the threshold.

### Queries

#### `get_pending_transactions(env, owner, from_index, limit)`

Returns paginated pending transactions.

#### `get_archived_transactions(env, owner, from_index, limit)`

Returns paginated archived transactions.

#### `get_multisig_config(env, tx_type)`

Returns the multi-sig configuration for a given transaction type.

#### `get_spending_tracker(env, member)`

Returns the spending tracker for a member.

### Emergency Controls

#### `set_emergency_config(env, owner, config)`

Sets the emergency transfer configuration. Owner only.

## Running Tests

```bash
cargo test -p family_wallet
```

