#![no_std]
//! # Insurance Contract
//!
//! This contract manages insurance policies with monthly premium payments.
//! It tracks policy status, coverage amounts, and premium payment schedules.
//!
//! ## Features
//! - Create insurance policies with configurable coverage
//! - Monthly premium payments
//! - Policy activation/deactivation
//! - Track payment schedules

use soroban_sdk::{
    contract, contractimpl, symbol_short, vec, Env, Map, Symbol, Vec, String,
};

/// Represents an insurance policy with coverage and premium details
///
/// # Fields
/// * `id` - Unique identifier for the policy
/// * `name` - Name/description of the policy
/// * `coverage_type` - Type of coverage (e.g., "health", "emergency", "education")
/// * `monthly_premium` - Monthly premium amount in stroops
/// * `coverage_amount` - Total coverage amount available
/// * `active` - Whether the policy is currently active
/// * `next_payment_date` - Unix timestamp of next premium payment due date
#[derive(Clone)]
#[contracttype]
pub struct InsurancePolicy {
    pub id: u32,
    pub name: String,
    pub coverage_type: String, // "health", "emergency", etc.
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub active: bool,
    pub next_payment_date: u64, // Unix timestamp
}

#[contract]
pub struct Insurance;

/// Smart contract for managing insurance policies and premium payments
///
/// This contract provides comprehensive insurance management functionality
/// including policy creation, premium payment tracking, and policy status management.
#[contraCreates a new insurance policy and sets the first payment due date
    /// to 30 days from the current block timestamp.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `name` - Name of the policy (e.g., "Family Health Insurance")
    /// * `coverage_type` - Type of coverage (e.g., "health", "emergency", "education")
    /// * `monthly_premium` - Monthly premium amount in stroops
    /// * `coverage_amount` - Total coverage amount available
    /// 
    /// # Returns
    /// The ID of the created policy as u32y premium amount
    /// * `coverage_amount` - Total coverage amount
    /// 
    /// # Returns
    /// The ID of the created policy
    pub fn create_policy(
        env: Env,
        name: String,
        coverage_type: String,
        monthly_premium: i128,
        coverage_amount: i128,
    ) -> u32 {
        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));
        
        let next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32)
            + 1;
        
        // Set next payment date to 30 days from now
        let next_payment_date = env.ledger().timestamp() + (30 * 86400);
        
        let policy = InsurancePolicy {
            id: next_id,
            name: name.clone(),
            coverage_type: coverage_type.clone(),
            monthly_premium,
            coverage_amount,
            active: true,
            next_payment_date,
        };
        
        policies.set(next_id, policy);
        env.storage().instance().set(&symbol_short!("POLICIES"), &policies);
        env.storage().instance().set(&symbol_short!("NEXT_ID"), &next_id);
        
        next_id
    }
    
    /// Pay monthly premium for a policy
    /// 
    /// Marks a monthly premium as paid and advances the next payment date
    /// to 30 days from the current block timestamp.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `policy_id` - ID of the policy to pay premium for
    /// 
    /// # Returns
    /// True if payment was successful, false if:
    /// - Policy not found
    /// - Policy is inactive
    ///
    /// # Error Codes
    /// - Implicit error: Policy not found (returns false)
    /// - Implicit error: Policy inactive (returns false)
    pub fn pay_premium(env: Env, policy_id: u32) -> bool {
        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));
        
        if let Some(mut policy) = policies.get(policy_id) {
            if !policy.active {
                return false; // Policy is not active
            }
            
            // Update next payment date to 30 days from now
            policy.next_payment_date = env.ledger().timestamp() + (30 * 86400);
            
            policies.set(policy_id, policy);
            env.storage().instance().set(&symbol_short!("POLICIES"), &policies);
            true
        } else {
            false
        }
    }
    
    /// Get a policy by ID
    /// 
    /// Retrieves a specific policy from storage.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `policy_id` - ID of the policy
    /// 
    /// # Returns
    /// Option<InsurancePolicy> - Some(policy) if found, None otherwise
    pub fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy> {
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));
        
        policies.get(policy_id)
    }
    
    /// Get all active policies
    /// 
    /// Retrieves all policies that are currently active.
    ///
    /// # Returns
    /// Vec<InsurancePolicy> - Vector of all active policies
    pub fn get_active_policies(env: Env) -> Vec<InsurancePolicy> {
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));
        
        let mut result = Vec::new(&env);
        let max_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32);
        
        for i in 1..=max_id {
            if let Some(policy) = policies.get(i) {
                if policy.active {
                    result.push_back(policy);
                }
            }
        }
        result
    }
    
    /// Get total monthly premium for all active policies
    /// 
    /// Sums the monthly premiums of all currently active policies.
    /// Useful for budgeting and cash flow analysis.
    ///
    /// # Returns
    /// i128 - Total monthly premium amount in stroops
    pub fn get_total_monthly_premium(env: Env) -> i128 {
        let active = Self::get_active_policies(env);
        let mut total = 0i128;
        for policy in active.iter() {
            total += policy.monthly_premium;
        }
        total
    }
    
    /// Deactivate a policy
    /// 
    /// Sets a policy's active status to false, preventing further premium payments.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `policy_id` - ID of the policy to deactivate
    /// 
    /// # Returns
    /// True if deactivation was successful, false if policy not found
    pub fn deactivate_policy(env: Env, policy_id: u32) -> bool {
        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));
        
        if let Some(mut policy) = policies.get(policy_id) {
            policy.active = false;
            policies.set(policy_id, policy);
            env.storage().instance().set(&symbol_short!("POLICIES"), &policies);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test;

