#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Map, Symbol, Vec,
};

// TTL constants — matches other Remitwise contracts
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280;
const INSTANCE_BUMP_AMOUNT: u32 = 518_400;

pub const CONTRACT_VERSION: u32 = 1;

/// String names for well-known protocol-wide config keys.
///
/// Use `Symbol::new(&env, config_keys::MAX_PAGE_LIMIT)` at call sites.
pub mod config_keys {
    /// Maximum items per page for all paginated queries.
    pub const MAX_PAGE_LIMIT: &str = "max_page_lmt";
    /// Default items per page when caller passes `limit = 0`.
    pub const DEFAULT_PAGE_LIMIT: &str = "def_page_lmt";
    /// Maximum items in a single batch write operation.
    pub const MAX_BATCH_SIZE: &str = "max_batch_sz";
    /// Maximum audit-log entries kept per entity.
    pub const MAX_AUDIT_ENTRIES: &str = "max_audit_en";
}

/// A typed config value stored under a Symbol key.
#[contracttype]
#[derive(Clone, Debug)]
pub enum ConfigValue {
    U32(u32),
    I128(i128),
    Bool(bool),
}

#[contracttype]
enum DataKey {
    Admin,
    Config,
    Version,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ConfigError {
    /// `initialize` has already been called.
    AlreadyInitialized = 1,
    /// Contract has not been initialized yet.
    NotInitialized = 2,
    /// Caller is not the admin.
    Unauthorized = 3,
}

#[contract]
pub struct GlobalConfig;

#[contractimpl]
impl GlobalConfig {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// One-time initialization. Sets `admin` and creates an empty config map.
    pub fn initialize(env: Env, admin: Address) -> Result<(), ConfigError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ConfigError::AlreadyInitialized);
        }
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Config, &Map::<Symbol, ConfigValue>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::Version, &CONTRACT_VERSION);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin management
    // -----------------------------------------------------------------------

    /// Return the current admin address, or `None` before initialization.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }

    /// Transfer admin rights. Only the current admin may call.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), ConfigError> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ConfigError::NotInitialized)?;
        if caller != admin {
            return Err(ConfigError::Unauthorized);
        }
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Config read / write
    // -----------------------------------------------------------------------

    /// Write a typed value under `key`. Only the admin may call.
    pub fn set_config(
        env: Env,
        caller: Address,
        key: Symbol,
        value: ConfigValue,
    ) -> Result<(), ConfigError> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ConfigError::NotInitialized)?;
        if caller != admin {
            return Err(ConfigError::Unauthorized);
        }
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let mut config: Map<Symbol, ConfigValue> = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(ConfigError::NotInitialized)?;
        config.set(key, value);
        env.storage().instance().set(&DataKey::Config, &config);
        Ok(())
    }

    /// Read a config value by key. Returns `None` if the key was never set.
    pub fn get_config(env: Env, key: Symbol) -> Option<ConfigValue> {
        let config: Map<Symbol, ConfigValue> = env.storage().instance().get(&DataKey::Config)?;
        config.get(key)
    }

    /// List every key that has been set.
    pub fn get_all_keys(env: Env) -> Vec<Symbol> {
        env.storage()
            .instance()
            .get::<_, Map<Symbol, ConfigValue>>(&DataKey::Config)
            .map(|c| c.keys())
            .unwrap_or_else(|| Vec::new(&env))
    }

    // -----------------------------------------------------------------------
    // Metadata
    // -----------------------------------------------------------------------

    pub fn version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Version)
            .unwrap_or(0)
    }
}
