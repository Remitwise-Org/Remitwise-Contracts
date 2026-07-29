#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Vec,
};

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

/// Insurance policy data structure with owner tracking for access control
#[derive(Clone)]
#[contracttype]
pub struct InsurancePolicy {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub coverage_type: String,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub active: bool,
    pub next_payment_date: u64,
}

/// Events emitted by the contract for audit trail
#[contracttype]
#[derive(Clone)]
pub enum InsuranceEvent {
    PolicyCreated,
    PremiumPaid,
    PolicyDeactivated,
    EmergencyShutdown,
    Resumed,
}

#[contract]
pub struct Insurance;

#[contractimpl]
impl Insurance {
    /// One-time setup for the address allowed to trigger emergency
    /// shutdown.
    ///
    /// # Panics
    /// - If the pause admin has already been set
    pub fn init_pause_admin(env: Env, admin: Address) {
        admin.require_auth();

        if env.storage().instance().has(&symbol_short!("PADMIN")) {
            panic!("Pause admin already initialized");
        }

        env.storage()
            .instance()
            .set(&symbol_short!("PADMIN"), &admin);
    }

    /// ## The emergency-shutdown flow
    ///
    /// Halts every state-changing action that creates a new financial
    /// commitment -- `create_policy` (new coverage) and `pay_premium`
    /// (money moving) both check `PAUSED` and refuse to run while it's
    /// set. Deliberately **not** blocked: `deactivate_policy`, so a policy
    /// owner can still exit their own coverage during a shutdown instead
    /// of being frozen into it, and every read-only getter, so the
    /// contract's state stays inspectable throughout.
    ///
    /// Only the address set by `init_pause_admin` may call this or
    /// `resume`. There is no timelock here (contrast with
    /// `bill_payments::finalize_admin_rotation`) -- the entire point of an
    /// emergency shutdown is to take effect immediately, in response to
    /// something already going wrong.
    ///
    /// # Panics
    /// - If the pause admin hasn't been initialized
    /// - If caller is not the pause admin
    pub fn emergency_shutdown(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_pause_admin(&env, &caller);

        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &true);
        env.events().publish(
            (symbol_short!("admin"), InsuranceEvent::EmergencyShutdown),
            caller,
        );
    }

    /// Lift a shutdown triggered by `emergency_shutdown`.
    ///
    /// # Panics
    /// - If the pause admin hasn't been initialized
    /// - If caller is not the pause admin
    pub fn resume(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_pause_admin(&env, &caller);

        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &false);
        env.events()
            .publish((symbol_short!("admin"), InsuranceEvent::Resumed), caller);
    }

    /// Whether the contract is currently in an emergency shutdown.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&symbol_short!("PAUSED"))
            .unwrap_or(false)
    }

    fn require_pause_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("PADMIN"))
            .expect("Pause admin not initialized");

        if &admin != caller {
            panic!("Only the pause admin can do this");
        }
    }

    fn require_not_paused(env: &Env) {
        if Self::is_paused(env.clone()) {
            panic!("Contract is in emergency shutdown");
        }
    }

    /// Create a new insurance policy
    ///
    /// # Arguments
    /// * `owner` - Address of the policy owner (must authorize)
    /// * `name` - Name of the policy
    /// * `coverage_type` - Type of coverage (e.g., "health", "emergency")
    /// * `monthly_premium` - Monthly premium amount (must be positive)
    /// * `coverage_amount` - Total coverage amount (must be positive)
    ///
    /// # Returns
    /// The ID of the created policy
    ///
    /// # Panics
    /// - If owner doesn't authorize the transaction
    /// - If monthly_premium is not positive
    /// - If coverage_amount is not positive
    pub fn create_policy(
        env: Env,
        owner: Address,
        name: String,
        coverage_type: String,
        monthly_premium: i128,
        coverage_amount: i128,
    ) -> u32 {
        Self::require_not_paused(&env);

        // Access control: require owner authorization
        owner.require_auth();

        // Input validation
        if monthly_premium <= 0 {
            panic!("Monthly premium must be positive");
        }
        if coverage_amount <= 0 {
            panic!("Coverage amount must be positive");
        }

        // Extend storage TTL
        Self::extend_instance_ttl(&env);

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
            owner: owner.clone(),
            name: name.clone(),
            coverage_type: coverage_type.clone(),
            monthly_premium,
            coverage_amount,
            active: true,
            next_payment_date,
        };

        let policy_owner = policy.owner.clone();
        policies.set(next_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PolicyCreated),
            (next_id, policy_owner),
        );

        next_id
    }

    /// Pay monthly premium for a policy
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the policy owner)
    /// * `policy_id` - ID of the policy
    ///
    /// # Returns
    /// True if payment was successful
    ///
    /// # Panics
    /// - If caller is not the policy owner
    /// - If policy is not found
    /// - If policy is not active
    pub fn pay_premium(env: Env, caller: Address, policy_id: u32) -> bool {
        Self::require_not_paused(&env);

        // Access control: require caller authorization
        caller.require_auth();

        // Extend storage TTL
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).expect("Policy not found");

        // Access control: verify caller is the owner
        if policy.owner != caller {
            panic!("Only the policy owner can pay premiums");
        }

        if !policy.active {
            panic!("Policy is not active");
        }

        // Update next payment date to 30 days from now
        policy.next_payment_date = env.ledger().timestamp() + (30 * 86400);

        policies.set(policy_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PremiumPaid),
            (policy_id, caller),
        );

        true
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

    /// Get all active policies for a specific owner
    ///
    /// # Arguments
    /// * `owner` - Address of the policy owner
    ///
    /// # Returns
    /// Vec of active InsurancePolicy structs belonging to the owner
    pub fn get_active_policies(env: Env, owner: Address) -> Vec<InsurancePolicy> {
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
                if policy.active && policy.owner == owner {
                    result.push_back(policy);
                }
            }
        }
        result
    }

    /// Get total monthly premium for all active policies of an owner
    ///
    /// # Arguments
    /// * `owner` - Address of the policy owner
    ///
    /// # Returns
    /// Total monthly premium amount for the owner's active policies
    pub fn get_total_monthly_premium(env: Env, owner: Address) -> i128 {
        let active = Self::get_active_policies(env, owner);
        let mut total = 0i128;
        for policy in active.iter() {
            total += policy.monthly_premium;
        }
        total
    }

    /// Deactivate a policy
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the policy owner)
    /// * `policy_id` - ID of the policy
    ///
    /// # Returns
    /// True if deactivation was successful
    ///
    /// # Panics
    /// - If caller is not the policy owner
    /// - If policy is not found
    pub fn deactivate_policy(env: Env, caller: Address, policy_id: u32) -> bool {
        // Access control: require caller authorization
        caller.require_auth();

        // Extend storage TTL
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).expect("Policy not found");

        // Access control: verify caller is the owner
        if policy.owner != caller {
            panic!("Only the policy owner can deactivate this policy");
        }

        policy.active = false;
        policies.set(policy_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PolicyDeactivated),
            (policy_id, caller),
        );

        true
    }

    /// Extend the TTL of instance storage
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn open_policy(env: &Env, client: &InsuranceClient, owner: &Address) -> u32 {
        client.create_policy(
            owner,
            &String::from_str(env, "Health"),
            &String::from_str(env, "health"),
            &100,
            &10_000,
        )
    }

    #[test]
    #[should_panic(expected = "Contract is in emergency shutdown")]
    fn emergency_shutdown_blocks_new_policies() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);

        let pause_admin = Address::generate(&env);
        let owner = Address::generate(&env);
        client.init_pause_admin(&pause_admin);

        client.emergency_shutdown(&pause_admin);
        assert!(client.is_paused());

        open_policy(&env, &client, &owner);
    }

    #[test]
    #[should_panic(expected = "Contract is in emergency shutdown")]
    fn emergency_shutdown_blocks_premium_payments() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);

        let pause_admin = Address::generate(&env);
        let owner = Address::generate(&env);
        client.init_pause_admin(&pause_admin);
        let policy_id = open_policy(&env, &client, &owner);

        client.emergency_shutdown(&pause_admin);
        client.pay_premium(&owner, &policy_id);
    }

    #[test]
    fn resume_allows_state_changes_again() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);

        let pause_admin = Address::generate(&env);
        let owner = Address::generate(&env);
        client.init_pause_admin(&pause_admin);

        client.emergency_shutdown(&pause_admin);
        client.resume(&pause_admin);

        assert!(!client.is_paused());
        open_policy(&env, &client, &owner); // does not panic
    }

    #[test]
    #[should_panic(expected = "Only the pause admin can do this")]
    fn only_the_pause_admin_can_shut_down() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);

        let pause_admin = Address::generate(&env);
        let stranger = Address::generate(&env);
        client.init_pause_admin(&pause_admin);

        client.emergency_shutdown(&stranger);
    }

    #[test]
    fn deactivate_policy_is_not_blocked_by_a_shutdown() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);

        let pause_admin = Address::generate(&env);
        let owner = Address::generate(&env);
        client.init_pause_admin(&pause_admin);
        let policy_id = open_policy(&env, &client, &owner);

        client.emergency_shutdown(&pause_admin);
        client.deactivate_policy(&owner, &policy_id); // does not panic

        assert!(!client.get_policy(&policy_id).unwrap().active);
    }
}
