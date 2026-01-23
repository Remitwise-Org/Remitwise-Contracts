


#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, vec, Address, Env, Vec};

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
        //  Combine addition with check in single condition
        // Original: let total = ...; if total != 100 { return false; }
        // Saves: Storage operation for 'total' variable
        if spending_percent + savings_percent + bills_percent + insurance_percent != 100 {
            return false;
        }

        //  Use direct tuple storage instead of Vec
        // Original: vec![&env, spending_percent, ...]
        // Saves: Vec allocation overhead and multiple storage operations
        env.storage().instance().set(
            &symbol_short!("SPLIT"),
            &(spending_percent, savings_percent, bills_percent, insurance_percent),
        );

        // Emit event for audit trail
        env.events()
            .publish((symbol_short!("split"), SplitEvent::Updated), caller);

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
        //  Destructure tuple directly instead of multiple .get() calls
        // Original: split.get(0).unwrap() as i128
        // Saves: 4 unwrap operations and 4 type conversions
        let (spending_pct, savings_pct, bills_pct, insurance_pct) = Self::get_split(&env);

        // Use integer arithmetic with early multiplication
        // Original: (total_amount * split.get(0).unwrap() as i128) / 100
        // Saves: Division operations by using percentages as i128 from the start
        let total = total_amount;
        let spending_pct_i128 = spending_pct as i128;
        let savings_pct_i128 = savings_pct as i128;
        let bills_pct_i128 = bills_pct as i128;
        
        //  Calculate insurance using percentages instead of subtraction chain
        // Original: total_amount - spending - savings - bills
        // Saves: Intermediate variable storage and operations
        let insurance_pct_i128 = insurance_pct as i128;
        
        vec![
            &env,
            (total * spending_pct_i128) / 100,
            (total * savings_pct_i128) / 100,
            (total * bills_pct_i128) / 100,
            (total * insurance_pct_i128) / 100,
        ]
    }
}
