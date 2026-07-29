# Configuration Schema Versioning

How we bump configuration schema versions to maintain backward compatibility.

## Overview

When the data structure of a contract configuration changes, we must ensure existing data remains readable or implement a migration path. This document defines the protocol for bumping schema versions.

## Protocol

1. **Versioning in Structure**: Configuration structs should use a version suffix (e.g., `ConfigV2`) or contain a `version: u32` field.
2. **Migration Entrypoint**: Contracts must implement a `migrate_config` entrypoint if a breaking change is introduced, or handle it via a `set_config` call that accepts the new version.
3. **Compatibility**: Maintain stability for older versions where possible.

## Example: Bumping a Config Struct

### Original Version (`v1`)

```rust
// In src/storage.rs or lib.rs
use soroban_sdk::{contracttype, Address, Symbol};

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigV1 {
    pub owner: Address,
    pub fee_percent: u32,
}

pub const STORAGE_KEY_CONFIG: Symbol = symbol_short!("CONFIG");
```

### New Version (`v2`)

Add a new field, e.g., `max_limit`.

```rust
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigV2 {
    pub owner: Address,
    pub fee_percent: u32,
    pub max_limit: i128,
}
```

### Migration Logic

```rust
use soroban_sdk::{Env, Symbol};

pub fn migrate_config(env: &Env) {
    // Read the old config using the existing storage key
    let old_config: ConfigV1 = env.storage().instance().get(&STORAGE_KEY_CONFIG).unwrap();

    // Map to new config structure with default value for the new field
    let new_config = ConfigV2 {
        owner: old_config.owner,
        fee_percent: old_config.fee_percent,
        max_limit: 1000_000_000, // Default value for v2
    };

    // Overwrite the storage with the new config structure
    env.storage().instance().set(&STORAGE_KEY_CONFIG, &new_config);
}
```

## Reviewer Checklist

- [ ] Does the new schema introduce breaking changes?
- [ ] Is there an explicit migration path?
- [ ] Have test cases been updated to cover v1 -> v2 migration?
- [ ] Does the migration function correctly set all new fields with appropriate defaults?
