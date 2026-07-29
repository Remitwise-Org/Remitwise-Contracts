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

## Automated Enforcement

Point 1 below is not just a review checklist item — it's enforced in CI.
[`testutils/tests/reserved_storage_keys_test.rs`](../testutils/tests/reserved_storage_keys_test.rs)
parses the table above directly out of this document (so the doc stays the
single source of truth) and cross-checks every reserved key against every
storage-key literal actually used in each contract's `src/lib.rs`, the same
way [`storage_key_source_scan_test.rs`](../testutils/tests/storage_key_source_scan_test.rs)
catches naming-convention drift. If a PR stores data under a reserved key,
this test fails the build instead of relying on a reviewer to spot it.

**Run it locally:**

```bash
cargo test --package testutils --test reserved_storage_keys_test -- --nocapture
```

When the future feature for a reserved key is actually implemented, remove
the row from the table above (see step 3 below) — the test will then stop
treating that key as reserved automatically, no test-code change required.

## Reviewer Verification

When reviewing PRs, verify that:
1. No `symbol_short!` macro invocation uses a key from the table above — automatically checked by `reserved_storage_keys_test.rs` (see [Automated Enforcement](#automated-enforcement)), but worth a manual glance for keys built up dynamically (e.g. `format!`) that the scanner can't see.
2. The storage layout tests in `testutils/tests/storage_key_naming_test.rs` are passing.
3. If the PR implements the future feature for a reserved key, the key is removed from this document and added to the [Storage Key Naming Conventions](storage-key-naming-conventions.md#common-storage-keys) table.

## References

- [Storage Key Naming Conventions](storage-key-naming-conventions.md) - Naming rules and CI-enforced conventions for all storage keys
- [Storage Layout Reference](../STORAGE_LAYOUT.md) - Complete storage layout documentation for every contract
- [testutils/tests/README.md](../testutils/tests/README.md) - Overview of all storage-key validation tests, including this document's enforcement test
