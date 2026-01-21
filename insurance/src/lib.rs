#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Env, Map, Vec, String,
};

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

#[contractimpl]
impl Insurance {
    /// Create a new insurance policy
    /// 
    /// # Arguments
    /// * `name` - Name of the policy
    /// * `coverage_type` - Type of coverage (e.g., "health", "emergency")
    /// * `monthly_premium` - Monthly premium amount
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
    /// # Arguments
    /// * `policy_id` - ID of the policy
    /// 
    /// # Returns
    /// True if payment was successful, false otherwise
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
    /// # Arguments
    /// * `policy_id` - ID of the policy
    /// 
    /// # Returns
    /// InsurancePolicy struct or None if not found
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
    /// # Returns
    /// Vec of active InsurancePolicy structs
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
    /// # Returns
    /// Total monthly premium amount
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
    /// # Arguments
    /// * `policy_id` - ID of the policy
    /// 
    /// # Returns
    /// True if deactivation was successful
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


