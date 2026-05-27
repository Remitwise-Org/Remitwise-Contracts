# Shared TTL-Bump Helpers

## Overview

`remitwise-common` provides three helper functions that centralise the canonical TTL extension policy for all contracts. Using these helpers instead of calling `extend_ttl` directly prevents the common mistake of swapping the `threshold` and `bump` arguments.

## Helpers

```rust
/// Extend the instance storage entry TTL (active data).
pub fn bump_instance(env: &Env)

/// Extend a persistent storage entry TTL.
pub fn bump_persistent<K: IntoVal<Env, Val>>(env: &Env, key: &K)

/// Extend the instance storage entry TTL using the archive window.
pub fn bump_archive(env: &Env)
```

## Constants Used

| Helper            | Threshold constant                        | Bump constant                      | Effective window                      |
| ----------------- | ----------------------------------------- | ---------------------------------- | ------------------------------------- |
| `bump_instance`   | `INSTANCE_LIFETIME_THRESHOLD` = 1 day     | `INSTANCE_BUMP_AMOUNT` = 30 days   | Extends to 30 days when TTL ≤ 1 day   |
| `bump_persistent` | `PERSISTENT_LIFETIME_THRESHOLD` = 15 days | `PERSISTENT_BUMP_AMOUNT` = 60 days | Extends to 60 days when TTL ≤ 15 days |
| `bump_archive`    | `ARCHIVE_LIFETIME_THRESHOLD` = 1 day      | `ARCHIVE_BUMP_AMOUNT` = 150 days   | Extends to 150 days when TTL ≤ 1 day  |

The invariant `threshold < bump` is asserted by constant tests in `remitwise-common/src/lib.rs` and by the helper unit tests.

## Usage

Import the helpers from `remitwise-common`:

```rust
use remitwise_common::{bump_instance, bump_archive, bump_persistent};
```

Call `bump_instance` on every state-changing operation:

```rust
pub fn my_mutating_fn(env: Env, ...) {
    // ... auth checks ...
    bump_instance(&env);   // keep instance alive for 30 days
    // ... write storage ...
}
```

Call `bump_archive` after writing to an archive map:

```rust
pub fn archive_old_transactions(env: Env, ...) {
    bump_instance(&env);   // active window
    // ... move rows to ARCH_TX ...
    bump_archive(&env);    // extend to 150-day archive window
}
```

Call `bump_persistent` when writing to a persistent storage key:

```rust
pub fn write_persistent_data(env: Env, key: &DataKey, value: &MyType) {
    env.storage().persistent().set(key, value);
    bump_persistent(&env, key);
}
```

## Why Not Inline `extend_ttl`?

Each contract previously called `extend_ttl` directly with its own local constants. This created two risks:

1. **Argument swap** — `extend_ttl(bump, threshold)` instead of `extend_ttl(threshold, bump)` silently sets a very short TTL (threshold) as the target, causing data to expire almost immediately.
2. **Drift** — local constants could diverge from the canonical values in `remitwise-common`, making the effective TTL policy inconsistent across contracts.

The helpers eliminate both risks by being the single source of truth.

## Tests

The helpers are tested in `remitwise-common/src/lib.rs`:

- `bump_instance_extends_instance_ttl` — verifies TTL ≥ `INSTANCE_BUMP_AMOUNT` after call
- `bump_archive_extends_instance_ttl_to_archive_amount` — verifies TTL ≥ `ARCHIVE_BUMP_AMOUNT` after call
- `bump_instance_threshold_less_than_bump_invariant` — constant ordering check
- `bump_persistent_threshold_less_than_bump_invariant` — constant ordering check
- `bump_archive_threshold_less_than_bump_invariant` — constant ordering check

## References

- `remitwise-common/src/lib.rs` — helper implementations and tests
- `STORAGE_LAYOUT.md` — per-contract TTL strategy
