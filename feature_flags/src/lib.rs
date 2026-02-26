#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Symbol,
};

/// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

/// Feature flag configuration
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeatureFlag {
    /// Unique key for the feature (e.g., "strict_goal_dates")
    pub key: String,
    /// Whether the feature is enabled
    pub enabled: bool,
    /// Optional description of what the feature does
    pub description: String,
    /// Timestamp when the flag was last updated
    pub updated_at: u64,
    /// Address that last updated the flag
    pub updated_by: Address,
}

/// Event emitted when a feature flag is updated
#[contracttype]
#[derive(Clone)]
pub struct FlagUpdatedEvent {
    pub key: String,
    pub enabled: bool,
    pub updated_by: Address,
    pub timestamp: u64,
}

#[contract]
pub struct FeatureFlagsContract;

#[contractimpl]
impl FeatureFlagsContract {
    const STORAGE_ADMIN: Symbol = symbol_short!("ADMIN");
    const STORAGE_FLAGS: Symbol = symbol_short!("FLAGS");
    const STORAGE_INITIALIZED: Symbol = symbol_short!("INIT");

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Self::STORAGE_ADMIN)
            .expect("Contract not initialized");

        if admin != *caller {
            panic!("Unauthorized: only admin can perform this action");
        }
    }

    fn get_flags_map(env: &Env) -> Map<String, FeatureFlag> {
        env.storage()
            .instance()
            .get(&Self::STORAGE_FLAGS)
            .unwrap_or_else(|| Map::new(env))
    }

    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    /// Initialize the feature flags contract with an admin
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();

        let initialized: bool = env
            .storage()
            .instance()
            .get(&Self::STORAGE_INITIALIZED)
            .unwrap_or(false);

        if initialized {
            panic!("Contract already initialized");
        }

        env.storage().instance().set(&Self::STORAGE_ADMIN, &admin);
        env.storage()
            .instance()
            .set(&Self::STORAGE_INITIALIZED, &true);
        env.storage()
            .instance()
            .set(&Self::STORAGE_FLAGS, &Map::<String, FeatureFlag>::new(&env));

        Self::extend_instance_ttl(&env);

        env.events()
            .publish((symbol_short!("flags"), symbol_short!("init")), admin);
    }

    // -----------------------------------------------------------------------
    // Admin operations
    // -----------------------------------------------------------------------

    /// Set or update a feature flag
    pub fn set_flag(env: Env, caller: Address, key: String, enabled: bool, description: String) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        Self::extend_instance_ttl(&env);

        if key.len() == 0 || key.len() > 32 {
            panic!("Flag key must be between 1 and 32 characters");
        }

        if description.len() > 256 {
            panic!("Description must be 256 characters or less");
        }

        let mut flags = Self::get_flags_map(&env);
        let timestamp = env.ledger().timestamp();

        let flag = FeatureFlag {
            key: key.clone(),
            enabled,
            description,
            updated_at: timestamp,
            updated_by: caller.clone(),
        };

        flags.set(key.clone(), flag);
        env.storage().instance().set(&Self::STORAGE_FLAGS, &flags);

        let event = FlagUpdatedEvent {
            key,
            enabled,
            updated_by: caller,
            timestamp,
        };

        env.events()
            .publish((symbol_short!("flags"), symbol_short!("updated")), event);
    }

    /// Remove a feature flag
    pub fn remove_flag(env: Env, caller: Address, key: String) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        Self::extend_instance_ttl(&env);

        let mut flags = Self::get_flags_map(&env);

        if !flags.contains_key(key.clone()) {
            panic!("Flag not found");
        }

        flags.remove(key.clone());
        env.storage().instance().set(&Self::STORAGE_FLAGS, &flags);

        env.events().publish(
            (symbol_short!("flags"), symbol_short!("removed")),
            (key, caller),
        );
    }

    /// Transfer admin role to a new address
    pub fn transfer_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        Self::extend_instance_ttl(&env);

        env.storage()
            .instance()
            .set(&Self::STORAGE_ADMIN, &new_admin);

        env.events().publish(
            (symbol_short!("flags"), symbol_short!("admin")),
            (caller, new_admin),
        );
    }

    // -----------------------------------------------------------------------
    // Query operations (public, no auth required)
    // -----------------------------------------------------------------------

    /// Check if a feature flag is enabled
    /// Returns false if the flag doesn't exist
    pub fn is_enabled(env: Env, key: String) -> bool {
        let flags = Self::get_flags_map(&env);

        match flags.get(key) {
            Some(flag) => flag.enabled,
            None => false,
        }
    }

    /// Get a specific feature flag
    pub fn get_flag(env: Env, key: String) -> Option<FeatureFlag> {
        let flags = Self::get_flags_map(&env);
        flags.get(key)
    }

    /// Get all feature flags
    pub fn get_all_flags(env: Env) -> Map<String, FeatureFlag> {
        Self::get_flags_map(&env)
    }

    /// Get the current admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Self::STORAGE_ADMIN)
            .expect("Contract not initialized")
    }

    /// Check if contract is initialized
    pub fn is_initialized(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&Self::STORAGE_INITIALIZED)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod test;
