# Reserved Storage Keys

**Audience: Contributors**

This document tracks storage keys in the Remitwise smart contracts that are **reserved for future use**. As a contributor, you must **not** use these keys for new features, temporary variables, or migrations. Using these keys will cause storage collisions when the planned features are rolled out.

## Why are these keys reserved?

We reserve keys ahead of time for roadmap features (e.g., yield generation, staking, advanced access control) to ensure that the storage layout remains contiguous and collision-free. Reviewers will reject PRs that attempt to store data under these namespaces.

## List of Reserved Keys

All reserved keys follow the [Storage Key Naming Conventions](storage-key-naming-conventions.md) (UPPERCASE_WITH_UNDERSCORES, max 9 characters).

| Reserved Key | Intended Future Feature | Do Not Use Because... |
|--------------|-------------------------|------------------------|
| `YIELD_CFG`  | Yield Generation V2 | Will store the configuration for external yield integration protocols. |
| `STAKE_POL`  | Staking & Rewards | Planned for the staking mechanism where users earn rewards on saved balances. |
| `REWARD_CF`  | Staking & Rewards | Reserved for the reward emission rate and token configuration. |
| `V2_MIGR`    | Next-Gen Migration | Reserved as a staging pointer for when the V2 contracts are deployed. |
| `TMP_LOCK`   | Advanced Time-Locks | Will handle multi-phase time-locks for family wallet large withdrawals. |

## Concrete Examples

If you are adding a feature for "Notification Preferences", do **not** use a reserved key like `REWARD_CF` or a generic key that might collide with future features.

**❌ Incorrect: Using a reserved or generic key**
```rust
use soroban_sdk::{symbol_short, Env, Symbol};

// DO NOT USE - this is a reserved key for staking features!
const KEY_REWARD: Symbol = symbol_short!("REWARD_CF"); 

pub fn set_notification(env: Env, enabled: bool) {
    env.storage().instance().set(&KEY_REWARD, &enabled);
}
```

**✅ Correct: Using an isolated, descriptive key**
```rust
use soroban_sdk::{symbol_short, Env, Symbol};

const KEY_NOTIF: Symbol = symbol_short!("NOTIF_CFG");

pub fn set_notification(env: Env, enabled: bool) {
    env.storage().instance().set(&KEY_NOTIF, &enabled);
}
```

## Reviewer Verification

When reviewing PRs, verify that:
1. No `symbol_short!` macro invocation uses a key from the table above.
2. The storage layout tests in `testutils/tests/storage_key_naming_test.rs` are passing.
3. If the PR implements the future feature for a reserved key, the key is removed from this document and added to the [Storage Key Naming Conventions](storage-key-naming-conventions.md#common-storage-keys) table.
