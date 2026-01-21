#![no_std]
//! # Remittance Split Contract
//!
//! This contract automatically distributes incoming remittances across
//! multiple accounts based on configurable percentage allocations.
//!
//! ## Features
//! - Configurable percentage splits (spending, savings, bills, insurance)
//! - Calculate split amounts from total remittance
//! - Default split configuration for new users

use soroban_sdk::{contract, contractimpl, symbol_short, vec, Env, Symbol, Vec};

#[contract]
pub struct RemittanceSplit;

/// Smart contract for splitting remittances across multiple financial purposes
///
/// This contract enables automatic distribution of incoming funds based on
/// user-defined percentages for different spending categories.
#[contractimpl]
impl RemittanceSplit {
    /// Initialize a remittance split configuration
    /// 
    /// Sets up the distribution percentages for splitting incoming remittances.
    /// All percentages must sum to exactly 100.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `spending_percent` - Percentage for daily spending (0-100)
    /// * `savings_percent` - Percentage for savings goals (0-100)
    /// * `bills_percent` - Percentage for bill payments (0-100)
    /// * `insurance_percent` - Percentage for insurance (0-100)
    /// 
    /// # Returns
    /// True if configuration successful, false if percentages don't sum to 100
    ///
    /// # Error Codes
    /// - Invalid total: percentages don't sum to 100 (returns false)
    ///
    /// # Example
    /// ```ignore
    /// // 50% spending, 30% savings, 15% bills, 5% insurance
    /// RemittanceSplit::initialize_split(env, 50, 30, 15, 5);
    /// ```
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
        
        // Store the split configuration
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
    /// Retrieves the current percentage distribution for remittance splitting.
    /// Returns default values if not yet configured.
    ///
    /// # Returns
    /// Vec<u32> - Percentages in order: [spending, savings, bills, insurance]
    /// 
    /// # Default Values
    /// If not configured: [50, 30, 15, 5]
    pub fn get_split(env: Env) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&symbol_short!("SPLIT"))
            .unwrap_or_else(|| vec![&env, 50, 30, 15, 5]) // Default split
    }
    
    /// Calculate split amounts from a total remittance amount
    /// 
    /// Applies the current split percentages to calculate individual amounts
    /// for each category. Handles rounding by allocating remainder to insurance.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `total_amount` - Total remittance amount in stroops
    /// 
    /// # Returns
    /// Vec<i128> - Amounts in order: [spending, savings, bills, insurance]
    ///
    /// # Example
    /// ```ignore
    /// let total = 100_000_000; // 10 USDC (8 decimals)
    /// let splits = RemittanceSplit::calculate_split(env, total);
    /// // Result: [50000000, 30000000, 15000000, 5000000]
    /// ```
    pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
        let split = Self::get_split(env);
        
        let spending = (total_amount * split.get(0).unwrap() as i128) / 100;
        let savings = (total_amount * split.get(1).unwrap() as i128) / 100;
        let bills = (total_amount * split.get(2).unwrap() as i128) / 100;
        let insurance = total_amount - spending - savings - bills; // Remainder to handle rounding
        
        vec![&env, spending, savings, bills, insurance]
    }
}

#[cfg(test)]
mod test;

