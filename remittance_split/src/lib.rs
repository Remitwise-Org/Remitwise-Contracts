#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, vec, Env, Vec};

#[contract]
pub struct RemittanceSplit;

#[contractimpl]
impl RemittanceSplit {
    /// Initialize a remittance split configuration
    ///
    /// # Arguments
    /// * `spending_percent` - Percentage allocated to spending (0-100)
    /// * `savings_percent` - Percentage allocated to savings (0-100)
    /// * `bills_percent` - Percentage allocated to bills (0-100)
    /// * `insurance_percent` - Percentage allocated to insurance (0-100)
    ///
    /// # Returns
    /// * `true` - Configuration was successfully set (percentages must sum to 100)
    /// * `false` - Percentages do not sum to 100
    ///
    /// # Errors
    /// No explicit errors; returns false if percentages don't sum to 100.
    pub fn initialize_split(
        env: Env,
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> bool {
        let total = spending_percent + savings_percent + bills_percent + insurance_percent;

        if total != 100 {
            return false;
        }

        env.storage().instance().set(
            &symbol_short!("SPLIT"),
            &vec![
                &env,
                spending_percent,
                savings_percent,
                bills_percent,
                insurance_percent,
            ],
        );

        true
    }

    /// Get the current split configuration
    ///
    /// # Returns
    /// A vector of four u32 values representing percentages: [spending, savings, bills, insurance].
    /// Returns default [50, 30, 15, 5] if not initialized.
    ///
    /// # Errors
    /// No errors; always returns a vector.
    pub fn get_split(env: &Env) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&symbol_short!("SPLIT"))
            .unwrap_or_else(|| vec![env, 50, 30, 15, 5])
    }

    /// Calculate split amounts from a total remittance amount
    ///
    /// # Arguments
    /// * `total_amount` - The total remittance amount to split
    ///
    /// # Returns
    /// A vector of four i128 values representing split amounts: [spending, savings, bills, insurance].
    /// The last amount (insurance) is calculated as remainder to ensure total sums correctly.
    ///
    /// # Errors
    /// No errors; always returns a vector.
    pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
        let split = Self::get_split(&env);

        let spending = (total_amount * split.get(0).unwrap() as i128) / 100;
        let savings = (total_amount * split.get(1).unwrap() as i128) / 100;
        let bills = (total_amount * split.get(2).unwrap() as i128) / 100;
        let insurance = total_amount - spending - savings - bills;

        vec![&env, spending, savings, bills, insurance]
    }
}
