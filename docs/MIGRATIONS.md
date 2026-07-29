# Contract Migrations and Schema Upgrades

This guide is for **contributors** making changes to Soroban contract specifications, storage keys, or state structures. It outlines how to add, modify, or remove fields without breaking backwards compatibility with existing ledger storage.

---

## 1. Soroban Storage and Serialization Overview

Soroban contracts persist state on-chain using key-value storage. Types annotated with `#[contracttype]` (such as structs and enums) are serialized into Stellar's native XDR (External Data Representation) format when stored.

When a contract is upgraded (i.e. replacing the WASM binary), the existing ledger entries are **not** automatically transformed. The upgraded contract must be able to deserialize existing data using its new code definition.

---

## 2. Backward-Compatible Struct Upgrades

### The Rule of Optional Fields
To add a new field to an existing `#[contracttype]` struct without breaking existing data:
1. **Always wrap new fields in `Option<T>`**.
2. When the upgraded contract deserializes an old ledger entry that lacks the new field, the Soroban deserializer will decode the missing field as `None`.
3. If you do not use `Option<T>` for a new field, deserializing existing entries will fail and panic, rendering the contract unusable.

### Example: Adding a Field

Suppose we have an existing configuration struct:

```rust
use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitConfig {
    pub owner: Address,
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
    pub usdc_contract: Address,
}
```

If we want to introduce a fee receiver address, we modify the struct as follows:

```rust
use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitConfig {
    pub owner: Address,
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
    pub usdc_contract: Address,
    // Safely added field
    pub fee_recipient: Option<Address>,
}
```

### Accessing Upgraded Fields in Code
When reading the configuration in the contract, always handle both `Some` and `None` states gracefully:

```rust
use soroban_sdk::{Env, Address, symbol_short};

pub fn get_fee_recipient(env: &Env) -> Option<Address> {
    let config: SplitConfig = env.storage().instance().get(&symbol_short!("CONFIG"))?;
    
    // Fall back to owner if fee_recipient has not been set (None)
    Some(config.fee_recipient.unwrap_or(config.owner))
}
```

---

## 3. Storage Key Rules

All storage keys must adhere to strict naming conventions to prevent collisions and maintain compatibility with the `symbol_short!` macro:

* **Maximum length:** 9 characters (macro constraint).
* **Case:** UPPERCASE_WITH_UNDERSCORES (e.g. `CONFIG`, `MEMBERS`, `PAUSE_ADM`).
* **Stability:** Never rename an existing storage key. Renaming a key causes the contract to look for data at a new key, leaving the old data orphaned and unreachable.

---

## 4. Writing State Migration Logic

If a schema change cannot be made backward-compatible (e.g., changing a field's underlying data type from `u32` to `u64`), you must write a migration function to programmatically translate existing storage.

### Step 1: Define Old and New Structs
Keep the definition of the older struct temporarily so you can deserialize the old format:

```rust
#[contracttype]
pub struct OldConfig {
    pub owner: Address,
    pub limit: u32,
}

#[contracttype]
pub struct NewConfig {
    pub owner: Address,
    pub limit: u64, // Type changed
}
```

### Step 2: Implement the `migrate` Entrypoint
Define a one-time administrative function to perform the migration:

```rust
use soroban_sdk::{contractimpl, Env, symbol_short, Address};

pub struct MyContract;

#[contractimpl]
impl MyContract {
    pub fn migrate(env: Env, caller: Address) -> Result<(), &'static str> {
        // 1. Authenticate the upgrade admin or owner
        caller.require_auth();
        
        // 2. Fetch and deserialize using the old struct layout
        let old_key = symbol_short!("CONFIG");
        let old_config: OldConfig = env
            .storage()
            .instance()
            .get(&old_key)
            .ok_or("Config not found")?;
            
        // 3. Transform data to the new layout
        let new_config = NewConfig {
            owner: old_config.owner,
            limit: old_config.limit as u64,
        };
        
        // 4. Overwrite storage with the new representation
        env.storage().instance().set(&old_key, &new_config);
        
        Ok(())
    }
}
```

> [!IMPORTANT]
> The `migrate` function must be called immediately after upgrading the contract WASM, before any other transaction invokes functions that interact with the migrated keys.

---

## 5. Local Verification Flow

Always verify your changes compile and pass validation locally before submitting a PR:

1. **Compile WASM Target:**
   ```bash
   cargo build --target wasm32-unknown-unknown --release
   ```
2. **Run Tests:**
   ```bash
   cargo test --workspace --all-targets
   ```
3. **Run Clippy Lint Checks:**
   ```bash
   cargo clippy --workspace --all-targets -- -D warnings
   ```
