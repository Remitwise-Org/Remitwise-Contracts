#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, vec, Env, Vec};

#[contract]
pub struct RemittanceSplit;

#[contractimpl]
impl RemittanceSplit {
    /// Initialize a remittance split configuration
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
    pub fn get_split(env: &Env) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&symbol_short!("SPLIT"))
            .unwrap_or_else(|| vec![env, 50, 30, 15, 5])
    }
    
    /// Calculate split amounts from a total remittance amount
    pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
        let split = Self::get_split(&env);
        
        let spending = (total_amount * split.get(0).unwrap() as i128) / 100;
        let savings = (total_amount * split.get(1).unwrap() as i128) / 100;
        let bills = (total_amount * split.get(2).unwrap() as i128) / 100;
        let insurance = total_amount - spending - savings - bills;
        
        vec![&env, spending, savings, bills, insurance]
    }
}

