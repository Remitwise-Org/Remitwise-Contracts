#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Symbol,
};

/// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

/// Event topics
const FLAG_SET: Symbol = symbol_short!("flag_set");
const FLAG_DEL: Symbol = symbol_short!("flag_del");

#[contract]
pub struct FeatureFlagsContract;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureFlag {
    pub key: String,
    pub enabled: bool,
    pub description: String,
    pub updated_at: u64,
    pub updated_by: Address,
}

#[contracttype]
#[derive(Clone)]
pub struct FlagSetEvent {
    pub key: String,
    pub enabled: bool,
    pub updated_by: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct FlagDeletedEvent {
    pub key: String,
    pub deleted_by: Address,
    pub timestamp: u64,
}

#[contractimpl]
impl FeatureFlagsContract {
    const STORAGE_ADMIN: Symbol = symbol_short!("ADMIN");
    const STORAGE_FLAGS: Symbol = symbol_short!("FLAGS");

    /// Initialize the feature flags contract with an admin
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();

        if env.storage().instance().has(&Self::STORAGE_ADMIN) {
            panic!("Contract already initialized");
        }

        env.storage().instance().set(&Self::STORAGE_ADMIN, &admin);
        
        // Initialize empty flags map
        let flags: Map<String, FeatureFlag> = Map::new(&env);
        env.storage().instance().set(&Self::STORAGE_FLAGS, &flags);

        Self::extend_instance_ttl(&env);
    }

    /// Set or update a feature flag (admin only)
    pub fn set_flag(env: Env, key: String, enabled: bool, description: String) {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let mut flags = Self::get_flags(&env);
        let timestamp = env.ledger().timestamp();

        let flag = FeatureFlag {
            key: key.clone(),
            enabled,
            description,
            updated_at: timestamp,
            updated_by: admin.clone(),
        };

        flags.set(key.clone(), flag);
        env.storage().instance().set(&Self::STORAGE_FLAGS, &flags);

        // Emit event
        env.events().publish(
            (FLAG_SET, key.clone()),
            FlagSetEvent {
                key,
                enabled,
                updated_by: admin,
                timestamp,
            },
        );

        Self::extend_instance_ttl(&env);
    }

    /// Check if a feature flag is enabled (public read)
    pub fn is_enabled(env: Env, key: String) -> bool {
        let flags = Self::get_flags(&env);
        
        match flags.get(key) {
            Some(flag) => flag.enabled,
            None => false, // Default to disabled if flag doesn't exist
        }
    }

    /// Get a feature flag details (public read)
    pub fn get_flag(env: Env, key: String) -> Option<FeatureFlag> {
        let flags = Self::get_flags(&env);
        flags.get(key)
    }

    /// Get all feature flags (public read)
    pub fn get_all_flags(env: Env) -> Map<String, FeatureFlag> {
        Self::get_flags(&env)
    }

    /// Delete a feature flag (admin only)
    pub fn delete_flag(env: Env, key: String) {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let mut flags = Self::get_flags(&env);
        
        if !flags.contains_key(key.clone()) {
            panic!("Feature flag not found");
        }

        flags.remove(key.clone());
        env.storage().instance().set(&Self::STORAGE_FLAGS, &flags);

        // Emit event
        env.events().publish(
            (FLAG_DEL, key.clone()),
            FlagDeletedEvent {
                key,
                deleted_by: admin,
                timestamp: env.ledger().timestamp(),
            },
        );

        Self::extend_instance_ttl(&env);
    }

    /// Update admin (current admin only)
    pub fn update_admin(env: Env, new_admin: Address) {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        env.storage().instance().set(&Self::STORAGE_ADMIN, &new_admin);
        Self::extend_instance_ttl(&env);
    }

    /// Get current admin
    pub fn get_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&Self::STORAGE_ADMIN)
            .expect("Contract not initialized")
    }

    // Helper functions

    fn get_flags(env: &Env) -> Map<String, FeatureFlag> {
        env.storage()
            .instance()
            .get(&Self::STORAGE_FLAGS)
            .unwrap_or_else(|| Map::new(env))
    }

    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }
}

#[cfg(test)]
mod test;
