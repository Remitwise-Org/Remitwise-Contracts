#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token::TokenClient, vec, Address, Env,
    Symbol, Vec,
};

#[derive(Clone)]
#[contracttype]
pub struct Allocation {
    pub category: Symbol,
    pub amount: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct AccountGroup {
    pub spending: Address,
    pub savings: Address,
    pub bills: Address,
    pub insurance: Address,
}

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

/// Split configuration with owner tracking for access control
#[derive(Clone)]
#[contracttype]
pub struct SplitConfig {
    pub owner: Address,
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
    pub initialized: bool,
}

/// Events emitted by the contract for audit trail
#[contracttype]
#[derive(Clone)]
pub enum SplitEvent {
    Initialized,
    Updated,
    Calculated,
}

#[contract]
pub struct RemittanceSplit;

#[contractimpl]
impl RemittanceSplit {
    /// Initialize a remittance split configuration
    ///
    /// # Arguments
    /// * `owner` - Address of the split owner (must authorize)
    /// * `spending_percent` - Percentage for spending (0-100)
    /// * `savings_percent` - Percentage for savings (0-100)
    /// * `bills_percent` - Percentage for bills (0-100)
    /// * `insurance_percent` - Percentage for insurance (0-100)
    ///
    /// # Returns
    /// True if initialization was successful
    ///
    /// # Panics
    /// - If owner doesn't authorize the transaction
    /// - If percentages don't sum to 100
    /// - If split is already initialized (use update_split instead)
    pub fn initialize_split(
        env: Env,
        owner: Address,
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> bool {
        // Verify owner authorization
        owner.require_auth();

        // Check if already initialized
        if env.storage().instance().has(&symbol_short!("CONFIG")) {
            panic!("Split already initialized, use update_split instead");
        }

        // Combine addition with check in single condition
        // Original: let total = ...; if total != 100 { return false; }
        // Saves: Storage operation for 'total' variable
        if spending_percent + savings_percent + bills_percent + insurance_percent != 100 {
            return false;
        }

        // Use direct tuple storage instead of Vec
        // Original: vec![&env, spending_percent, ...]
        // Saves: Vec allocation overhead and multiple storage operations
        env.storage().instance().set(
            &symbol_short!("SPLIT"),
            &(spending_percent, savings_percent, bills_percent, insurance_percent),
        );

        // Store config with owner information
        let config = SplitConfig {
            owner: owner.clone(),
            spending_percent,
            savings_percent,
            bills_percent,
            insurance_percent,
            initialized: true,
        };
        env.storage().instance().set(&symbol_short!("CONFIG"), &config);

        // Extend TTL for instance storage
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        // Emit event for audit trail
        env.events()
            .publish((symbol_short!("split"), SplitEvent::Initialized), owner);

        true
    }

    /// Update an existing split configuration
    ///
    /// # Arguments
    /// * `owner` - Address of the split owner (must match stored owner and authorize)
    /// * `spending_percent` - New percentage for spending (0-100)
    /// * `savings_percent` - New percentage for savings (0-100)
    /// * `bills_percent` - New percentage for bills (0-100)
    /// * `insurance_percent` - New percentage for insurance (0-100)
    ///
    /// # Returns
    /// True if update was successful
    pub fn update_split(
        env: Env,
        owner: Address,
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> bool {
        // Verify owner authorization
        owner.require_auth();

        // Check if initialized
        let config: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .unwrap_or_else(|| panic!("Split not initialized"));

        // Verify caller is the owner
        if config.owner != owner {
            panic!("Only owner can update split configuration");
        }

        // Validate percentages sum to 100
        if spending_percent + savings_percent + bills_percent + insurance_percent != 100 {
            return false;
        }

        // Update storage
        env.storage().instance().set(
            &symbol_short!("SPLIT"),
            &(spending_percent, savings_percent, bills_percent, insurance_percent),
        );

        // Update config
        let updated_config = SplitConfig {
            owner: owner.clone(),
            spending_percent,
            savings_percent,
            bills_percent,
            insurance_percent,
            initialized: true,
        };
        env.storage().instance().set(&symbol_short!("CONFIG"), &updated_config);

        // Extend TTL
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        // Emit event
        env.events()
            .publish((symbol_short!("split"), SplitEvent::Updated), owner);

        true
    }

    /// Get the current split configuration
    pub fn get_split(env: &Env) -> (u32, u32, u32, u32) {
        // Return tuple instead of Vec for direct access
        // Original: Returns Vec<u32> which requires allocation
        // Saves: Vec allocation and provides compile-time type safety
        env.storage()
            .instance()
            .get(&symbol_short!("SPLIT"))
            .unwrap_or_else(|| (50, 30, 15, 5))
    }

    /// Get the full split configuration including owner
    ///
    /// # Returns
    /// SplitConfig or None if not initialized
    pub fn get_config(env: Env) -> Option<SplitConfig> {
        env.storage().instance().get(&symbol_short!("CONFIG"))
    }

    /// Calculate split amounts from a total remittance amount
    ///
    /// # Arguments
    /// * `total_amount` - The total amount to split (must be positive)
    ///
    /// # Returns
    /// Vec containing [spending, savings, bills, insurance] amounts
    ///
    /// # Panics
    /// - If total_amount is not positive
    pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
        // Validate input
        if total_amount <= 0 {
            panic!("Total amount must be positive");
        }

// Destructure tuple directly instead of multiple .get() calls
let (spending_pct, savings_pct, bills_pct, _) = Self::get_split(&env);

let total = total_amount;

// Calculate splits using integer arithmetic
let spending = (total * spending_pct as i128) / 100;
let savings  = (total * savings_pct as i128) / 100;
let bills    = (total * bills_pct as i128) / 100;

// Insurance gets the remainder to avoid rounding issues
let insurance = total - spending - savings - bills;

// Emit event
env.events().publish(
    (symbol_short!("split"), SplitEvent::Calculated),
    total_amount,
);

vec![&env, spending, savings, bills, insurance]
  }
  
}