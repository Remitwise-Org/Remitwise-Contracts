#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Env, Map, String, Symbol, Vec};

// Event topics
const POLICY_CREATED: Symbol = symbol_short!("created");
const PREMIUM_PAID: Symbol = symbol_short!("paid");
const POLICY_DEACTIVATED: Symbol = symbol_short!("deactive");

// Event data structures
#[derive(Clone)]
#[contracttype]
pub struct PolicyCreatedEvent {
    pub policy_id: u32,
    pub name: String,
    pub coverage_type: String,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct PremiumPaidEvent {
    pub policy_id: u32,
    pub name: String,
    pub amount: i128,
    pub next_payment_date: u64,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct PolicyDeactivatedEvent {
    pub policy_id: u32,
    pub name: String,
    pub timestamp: u64,
}

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
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);

        // Emit PolicyCreated event
        let event = PolicyCreatedEvent {
            policy_id: next_id,
            name: name.clone(),
            coverage_type: coverage_type.clone(),
            monthly_premium,
            coverage_amount,
            timestamp: env.ledger().timestamp(),
        };
        env.events().publish((POLICY_CREATED,), event);

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

            // Emit PremiumPaid event
            let event = PremiumPaidEvent {
                policy_id,
                name: policy.name.clone(),
                amount: policy.monthly_premium,
                next_payment_date: policy.next_payment_date,
                timestamp: env.ledger().timestamp(),
            };
            env.events().publish((PREMIUM_PAID,), event);

            policies.set(policy_id, policy);
            env.storage()
                .instance()
                .set(&symbol_short!("POLICIES"), &policies);
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

            // Emit PolicyDeactivated event
            let event = PolicyDeactivatedEvent {
                policy_id,
                name: policy.name.clone(),
                timestamp: env.ledger().timestamp(),
            };
            env.events().publish((POLICY_DEACTIVATED,), event);

            policies.set(policy_id, policy);
            env.storage()
                .instance()
                .set(&symbol_short!("POLICIES"), &policies);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Events;

    #[test]
    fn test_create_policy_emits_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);

        // Create a policy
        let policy_id = client.create_policy(
            &String::from_str(&env, "Health Insurance"),
            &String::from_str(&env, "health"),
            &100,
            &50000,
        );
        assert_eq!(policy_id, 1);

        // Verify event was emitted
        let events = env.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_pay_premium_emits_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);

        // Create a policy
        let policy_id = client.create_policy(
            &String::from_str(&env, "Emergency Coverage"),
            &String::from_str(&env, "emergency"),
            &75,
            &25000,
        );

        // Get events before paying premium
        let events_before = env.events().all().len();

        // Pay premium
        let result = client.pay_premium(&policy_id);
        assert!(result);

        // Verify PremiumPaid event was emitted (1 new event)
        let events_after = env.events().all().len();
        assert_eq!(events_after - events_before, 1);
    }

    #[test]
    fn test_deactivate_policy_emits_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);

        // Create a policy
        let policy_id = client.create_policy(
            &String::from_str(&env, "Life Insurance"),
            &String::from_str(&env, "life"),
            &200,
            &100000,
        );

        // Get events before deactivating
        let events_before = env.events().all().len();

        // Deactivate policy
        let result = client.deactivate_policy(&policy_id);
        assert!(result);

        // Verify PolicyDeactivated event was emitted (1 new event)
        let events_after = env.events().all().len();
        assert_eq!(events_after - events_before, 1);
    }

    #[test]
    fn test_multiple_policies_emit_separate_events() {
        let env = Env::default();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);

        // Create multiple policies
        client.create_policy(
            &String::from_str(&env, "Policy 1"),
            &String::from_str(&env, "health"),
            &100,
            &50000,
        );
        client.create_policy(
            &String::from_str(&env, "Policy 2"),
            &String::from_str(&env, "life"),
            &200,
            &100000,
        );
        client.create_policy(
            &String::from_str(&env, "Policy 3"),
            &String::from_str(&env, "emergency"),
            &75,
            &25000,
        );

        // Should have 3 PolicyCreated events
        let events = env.events().all();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_policy_lifecycle_emits_all_events() {
        let env = Env::default();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);

        // Create a policy
        let policy_id = client.create_policy(
            &String::from_str(&env, "Complete Lifecycle"),
            &String::from_str(&env, "health"),
            &150,
            &75000,
        );

        // Pay premium
        client.pay_premium(&policy_id);

        // Deactivate
        client.deactivate_policy(&policy_id);

        // Should have 3 events: Created, PremiumPaid, Deactivated
        let events = env.events().all();
        assert_eq!(events.len(), 3);
    }
}
