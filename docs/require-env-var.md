# `require_env_var` — Required Per-Contract Config Helper

**Crate:** `remitwise-common`  
**Issue:** #1143  
**Status:** Stable

---

## Motivation

Contracts often store optional configuration values in instance storage
(feature flags, version numbers, dependency addresses, caps, etc.). Call
sites historically used `env.storage().instance().get(...).unwrap_or(default)`
or ad-hoc `Option` checks, which:

1. Silently defaulted when a required value was missing.
2. Forced each contract to invent its own missing-key error.
3. Made operator and frontend debugging harder ("why is this zero?").

`require_env_var` reads a required configuration value from instance storage
and returns a clear `EnvVarError::Missing` when the key is absent.

---

## API

```rust
#[contracterror]
#[repr(u32)]
pub enum EnvVarError {
    /// The requested configuration value is not set in instance storage.
    Missing = 100,
}

pub fn require_env_var<T>(env: &Env, key: &Symbol) -> Result<T, EnvVarError>
where
    T: TryFromVal<Env, Val>,
```

### Arguments

| Parameter | Type       | Description                                                                 |
|-----------|------------|-----------------------------------------------------------------------------|
| `env`     | `&Env`     | Soroban environment.                                                        |
| `key`     | `&Symbol`  | Storage key (typically from `symbol_short!` or `Symbol::new`).              |

### Type parameter

`T` must implement `TryFromVal<Env, Val>`, so any Soroban-storable type works
(`bool`, `u32`, `i128`, `Address`, `Symbol`, contracttypes, etc.).

### Returns

| Result | Meaning |
|--------|---------|
| `Ok(T)` | Value found in instance storage. |
| `Err(EnvVarError::Missing)` | Key is absent. |

---

## Usage

```rust
use remitwise_common::{require_env_var, EnvVarError};
use soroban_sdk::{symbol_short, panic_with_error};

// At init / admin set:
env.storage().instance().set(&symbol_short!("MAX_AMNT"), &1_000_000i128);

// At read sites that require the value:
let max: i128 = require_env_var(&env, &symbol_short!("MAX_AMNT"))
    .unwrap_or_else(|e| panic_with_error!(&env, e));
```

For `Result`-returning entry points that map into a contract-local error:

```rust
let admin: Address = require_env_var(&env, &symbol_short!("ADMIN"))
    .map_err(|_| MyError::NotConfigured)?;
```

---

## Design notes

1. **Backwards compatible** — additive API; existing helpers are unchanged.
2. **`#![no_std]`** — uses only `soroban_sdk` primitives.
3. **Clear failure** — does not silently default; callers choose how to surface
   `EnvVarError::Missing` (`panic_with_error!` or map into a local error).
4. **Generic** — one helper covers all storable config types instead of
   per-type wrappers.

---

## Tests

Covered in `remitwise-common/src/tests.rs`:

| Test | Asserts |
|------|---------|
| `test_require_env_var_u32_success` | Reads stored `u32` |
| `test_require_env_var_bool_success` | Reads stored `bool` |
| `test_require_env_var_i128_success` | Reads stored `i128` |
| `test_require_env_var_address_success` | Reads stored `Address` |
| `test_require_env_var_missing` | Returns `Err(Missing)` when absent |
| `test_require_env_var_different_key_same_env` | Presence vs absence in same env |

```bash
cargo test -p remitwise-common -- test_require_env_var
```
