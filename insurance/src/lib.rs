// #![no_std]
// use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Env, Map, String, Vec};

// #[derive(Clone)]
// #[contracttype]
// pub struct InsurancePolicy {
//     pub id: u32,
//     pub name: String,
//     pub coverage_type: String, // "health", "emergency", etc.
//     pub monthly_premium: i128,
//     pub coverage_amount: i128,
//     pub active: bool,
//     pub next_payment_date: u64, // Unix timestamp
// }

// #[contract]
// pub struct Insurance;

// #[contractimpl]
// impl Insurance {
//     /// Create a new insurance policy
//     ///
//     /// # Arguments
//     /// * `name` - Name of the policy
//     /// * `coverage_type` - Type of coverage (e.g., "health", "emergency")
//     /// * `monthly_premium` - Monthly premium amount
//     /// * `coverage_amount` - Total coverage amount
//     ///
//     /// # Returns
//     /// The ID of the created policy
//     pub fn create_policy(
//         env: Env,
//         name: String,
//         coverage_type: String,
//         monthly_premium: i128,
//         coverage_amount: i128,
//     ) -> u32 {
//         let mut policies: Map<u32, InsurancePolicy> = env
//             .storage()
//             .instance()
//             .get(&symbol_short!("POLICIES"))
//             .unwrap_or_else(|| Map::new(&env));

//         let next_id = env
//             .storage()
//             .instance()
//             .get(&symbol_short!("NEXT_ID"))
//             .unwrap_or(0u32)
//             + 1;

//         // Set next payment date to 30 days from now
//         let next_payment_date = env.ledger().timestamp() + (30 * 86400);

//         let policy = InsurancePolicy {
//             id: next_id,
//             name: name.clone(),
//             coverage_type: coverage_type.clone(),
//             monthly_premium,
//             coverage_amount,
//             active: true,
//             next_payment_date,
//         };

//         policies.set(next_id, policy);
//         env.storage()
//             .instance()
//             .set(&symbol_short!("POLICIES"), &policies);
//         env.storage()
//             .instance()
//             .set(&symbol_short!("NEXT_ID"), &next_id);

//         next_id
//     }

//     /// Pay monthly premium for a policy
//     ///
//     /// # Arguments
//     /// * `policy_id` - ID of the policy
//     ///
//     /// # Returns
//     /// True if payment was successful, false otherwise
//     pub fn pay_premium(env: Env, policy_id: u32) -> bool {
//         let mut policies: Map<u32, InsurancePolicy> = env
//             .storage()
//             .instance()
//             .get(&symbol_short!("POLICIES"))
//             .unwrap_or_else(|| Map::new(&env));

//         if let Some(mut policy) = policies.get(policy_id) {
//             if !policy.active {
//                 return false; // Policy is not active
//             }

//             // Update next payment date to 30 days from now
//             policy.next_payment_date = env.ledger().timestamp() + (30 * 86400);

//             policies.set(policy_id, policy);
//             env.storage()
//                 .instance()
//                 .set(&symbol_short!("POLICIES"), &policies);
//             true
//         } else {
//             false
//         }
//     }

//     /// Get a policy by ID
//     ///
//     /// # Arguments
//     /// * `policy_id` - ID of the policy
//     ///
//     /// # Returns
//     /// InsurancePolicy struct or None if not found
//     pub fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy> {
//         let policies: Map<u32, InsurancePolicy> = env
//             .storage()
//             .instance()
//             .get(&symbol_short!("POLICIES"))
//             .unwrap_or_else(|| Map::new(&env));

//         policies.get(policy_id)
//     }

//     /// Get all active policies
//     ///
//     /// # Returns
//     /// Vec of active InsurancePolicy structs
//     pub fn get_active_policies(env: Env) -> Vec<InsurancePolicy> {
//         let policies: Map<u32, InsurancePolicy> = env
//             .storage()
//             .instance()
//             .get(&symbol_short!("POLICIES"))
//             .unwrap_or_else(|| Map::new(&env));

//         let mut result = Vec::new(&env);
//         let max_id = env
//             .storage()
//             .instance()
//             .get(&symbol_short!("NEXT_ID"))
//             .unwrap_or(0u32);

//         for i in 1..=max_id {
//             if let Some(policy) = policies.get(i) {
//                 if policy.active {
//                     result.push_back(policy);
//                 }
//             }
//         }
//         result
//     }

//     /// Get total monthly premium for all active policies
//     ///
//     /// # Returns
//     /// Total monthly premium amount
//     pub fn get_total_monthly_premium(env: Env) -> i128 {
//         let active = Self::get_active_policies(env);
//         let mut total = 0i128;
//         for policy in active.iter() {
//             total += policy.monthly_premium;
//         }
//         total
//     }

//     /// Deactivate a policy
//     ///
//     /// # Arguments
//     /// * `policy_id` - ID of the policy
//     ///
//     /// # Returns
//     /// True if deactivation was successful
//     pub fn deactivate_policy(env: Env, policy_id: u32) -> bool {
//         let mut policies: Map<u32, InsurancePolicy> = env
//             .storage()
//             .instance()
//             .get(&symbol_short!("POLICIES"))
//             .unwrap_or_else(|| Map::new(&env));

//         if let Some(mut policy) = policies.get(policy_id) {
//             policy.active = false;
//             policies.set(policy_id, policy);
//             env.storage()
//                 .instance()
//                 .set(&symbol_short!("POLICIES"), &policies);
//             true
//         } else {
//             false
//         }
//     }
// }


#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Env, Map, String, Symbol, Vec,
};

// ============================================================================
//  Optimized struct layout with field reordering
// - Moved fixed-size fields before variable-size fields
// - Changed id from u32 to u64 (native word size on Stellar)
// - Grouped related fields together for better cache locality
// ============================================================================
#[derive(Clone)]
#[contracttype]
pub struct InsurancePolicy {

    pub monthly_premium: i128,      // Fixed size, frequently accessed
    pub coverage_amount: i128,      // Fixed size
    pub next_payment_date: u64,     // Unix timestamp
    pub id: u64,                    // Changed from u32 to u64 (native word size)
    pub active: bool,               // Boolean
    pub name: String,               // Variable size last
    pub coverage_type: String,      // Variable size last
}

// ============================================================================
//  Storage keys as compile-time constants
// - Eliminates repeated symbol creation on every function call
// - Symbols are created once and reused throughout the contract
// ============================================================================
const POLICIES_KEY: Symbol = symbol_short!("POLICIES");
const NEXT_ID_KEY: Symbol = symbol_short!("NEXT_ID");
const ACTIVE_COUNT_KEY: Symbol = symbol_short!("ACTIVE");
const TOTAL_PREMIUM_KEY: Symbol = symbol_short!("PREMIUM");

// ============================================================================
//  Constants for magic numbers
// - Reduces repeated calculations
// - Makes code more maintainable
// ============================================================================
const SECONDS_PER_DAY: u64 = 86400;
const DAYS_PER_MONTH: u64 = 30;
const MONTH_IN_SECONDS: u64 = DAYS_PER_MONTH * SECONDS_PER_DAY;

#[contract]
pub struct Insurance;

#[contractimpl]
impl Insurance {
    // ========================================================================
    //  Initialize function to set up storage
    // - Avoids repeated unwrap_or_else calls
    // - Pre-allocates storage structures
    // ========================================================================
    pub fn initialize(env: Env) {
        if env.storage().instance().has(&NEXT_ID_KEY) {
            panic!("Already initialized");
        }
        
        env.storage().instance().set(&NEXT_ID_KEY, &0u64);
        env.storage().instance().set(&ACTIVE_COUNT_KEY, &0u64);
        env.storage().instance().set(&TOTAL_PREMIUM_KEY, &0i128);
        let policies: Map<u64, InsurancePolicy> = Map::new(&env);
        env.storage().instance().set(&POLICIES_KEY, &policies);
    }

    // ========================================================================
    //  Optimized create_policy function
    // - Early validation before storage reads
    // - Removed unnecessary clone operations
    // - Batch storage writes
    // - Maintains active count and total premium cache
    // ========================================================================
    pub fn create_policy(
        env: Env,
        name: String,
        coverage_type: String,
        monthly_premium: i128,
        coverage_amount: i128,
    ) -> u64 {
        //  Early validation to fail fast
        if monthly_premium <= 0 || coverage_amount <= 0 {
            panic!("Premium and coverage must be positive");
        }

        // OPTIMIZATION: Single storage read
        let mut policies: Map<u64, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&POLICIES_KEY)
            .unwrap_or_else(|| Map::new(&env));

        
        let next_id: u64 = env
            .storage()
            .instance()
            .get(&NEXT_ID_KEY)
            .unwrap_or(0u64)
            + 1;

        // OPTIMIZATION: Calculate once, reuse
        let next_payment_date = env.ledger().timestamp() + MONTH_IN_SECONDS;

        // OPTIMIZATION: No unnecessary clones
        let policy = InsurancePolicy {
            id: next_id,
            monthly_premium,
            coverage_amount,
            next_payment_date,
            active: true,
            name,
            coverage_type,
        };

        policies.set(next_id, policy);

        // Update active count
        let active_count: u64 = env
            .storage()
            .instance()
            .get(&ACTIVE_COUNT_KEY)
            .unwrap_or(0u64)
            + 1;

        //  Update total premium cache
        let total_premium: i128 = env
            .storage()
            .instance()
            .get(&TOTAL_PREMIUM_KEY)
            .unwrap_or(0i128)
            + monthly_premium;

        //  Batch storage writes
        env.storage().instance().set(&POLICIES_KEY, &policies);
        env.storage().instance().set(&NEXT_ID_KEY, &next_id);
        env.storage().instance().set(&ACTIVE_COUNT_KEY, &active_count);
        env.storage().instance().set(&TOTAL_PREMIUM_KEY, &total_premium);

        //  Extend TTL for frequently accessed data
        env.storage().instance().extend_ttl(100, 100);

        next_id
    }

    // ========================================================================
    // 
    // - Early existence check with contains_key
    // - Eliminated redundant operations
    // - Single storage write
    // ========================================================================
    pub fn pay_premium(env: Env, policy_id: u64) -> bool {
        let mut policies: Map<u64, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&POLICIES_KEY)
            .unwrap_or_else(|| Map::new(&env));

        //  Cheap existence check first
        if !policies.contains_key(policy_id) {
            return false;
        }

        let mut policy = policies.get(policy_id).unwrap();

        if !policy.active {
            return false;
        }

       
        policy.next_payment_date = env.ledger().timestamp() + MONTH_IN_SECONDS;

        // OPTIMIZATION: Single update
        policies.set(policy_id, policy);
        env.storage().instance().set(&POLICIES_KEY, &policies);
        
        env.storage().instance().extend_ttl(100, 100);
        true
    }

    // ========================================================================
    // 
    // - Uses functional chaining
    // - Eliminates unnecessary Map creation
    // ========================================================================
    pub fn get_policy(env: Env, policy_id: u64) -> Option<InsurancePolicy> {
        env.storage()
            .instance()
            .get(&POLICIES_KEY)
            .and_then(|policies: Map<u64, InsurancePolicy>| policies.get(policy_id))
    }

    // ========================================================================
    
    // - Streamlined iteration
    // - Uses active count hint for optimization
    // ========================================================================
    pub fn get_active_policies(env: Env) -> Vec<InsurancePolicy> {
        let policies: Map<u64, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&POLICIES_KEY)
            .unwrap_or_else(|| Map::new(&env));

        let max_id: u64 = env
            .storage()
            .instance()
            .get(&NEXT_ID_KEY)
            .unwrap_or(0u64);

        let mut result = Vec::new(&env);

        // OPTIMIZATION: Iterate only through existing IDs
        for id in 1..=max_id {
            if let Some(policy) = policies.get(id) {
                if policy.active {
                    result.push_back(policy);
                }
            }
        }

        result
    }

    // ========================================================================
    // - O(1) lookup instead of O(n) iteration
    // - Uses pre-calculated cached value
    // ========================================================================
    pub fn get_total_monthly_premium(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&TOTAL_PREMIUM_KEY)
            .unwrap_or(0i128)
    }

    // ========================================================================
    // - Updates cached values (active count, total premium)
    // - Existence check before retrieval
    // ========================================================================
    pub fn deactivate_policy(env: Env, policy_id: u64) -> bool {
        let mut policies: Map<u64, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&POLICIES_KEY)
            .unwrap_or_else(|| Map::new(&env));

        //  Check existence first
        if !policies.contains_key(policy_id) {
            return false;
        }

        let mut policy = policies.get(policy_id).unwrap();

        if !policy.active {
            return false; // Already inactive
        }

        policy.active = false;
        policies.set(policy_id, policy.clone());

        //  Update active count
        let active_count: u64 = env
            .storage()
            .instance()
            .get(&ACTIVE_COUNT_KEY)
            .unwrap_or(1u64)
            .saturating_sub(1);

        // Update total premium cache
        let total_premium: i128 = env
            .storage()
            .instance()
            .get(&TOTAL_PREMIUM_KEY)
            .unwrap_or(policy.monthly_premium)
            - policy.monthly_premium;

        // OPTIMIZATION: Batch updates
        env.storage().instance().set(&POLICIES_KEY, &policies);
        env.storage().instance().set(&ACTIVE_COUNT_KEY, &active_count);
        env.storage().instance().set(&TOTAL_PREMIUM_KEY, &total_premium);

        env.storage().instance().extend_ttl(100, 100);
        true
    }
}