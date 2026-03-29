#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, String,
    Symbol, Vec,
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Symbol, Vec,
};

use remitwise_common::CoverageType;
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum InsuranceError {
    PolicyNotFound = 1,
    Unauthorized = 2,
    InvalidAmount = 3,
    PolicyInactive = 4,
    ContractPaused = 5,
    FunctionPaused = 6,
    InvalidTimestamp = 7,
    BatchTooLarge = 8,
}

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
    pub coverage_type: CoverageType,
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

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

const CONTRACT_VERSION: u32 = 1;
const MAX_BATCH_SIZE: u32 = 50;
const STORAGE_PREMIUM_TOTALS: Symbol = symbol_short!("PRM_TOT");

/// Pagination constants
pub const DEFAULT_PAGE_LIMIT: u32 = 20;
pub const MAX_PAGE_LIMIT: u32 = 50;

pub mod pause_functions {
    use soroban_sdk::{symbol_short, Symbol};
    pub const CREATE_POLICY: Symbol = symbol_short!("crt_pol");
    pub const PAY_PREMIUM: Symbol = symbol_short!("pay_prem");
    pub const DEACTIVATE: Symbol = symbol_short!("deact");
    pub const CREATE_SCHED: Symbol = symbol_short!("crt_sch");
    pub const MODIFY_SCHED: Symbol = symbol_short!("mod_sch");
    pub const CANCEL_SCHED: Symbol = symbol_short!("can_sch");
}

/// Insurance policy data structure with owner tracking for access control
#[derive(Clone)]
#[contracttype]
#[derive(Clone)]
#[contracttype]
#[derive(Clone)]
#[contracttype]
pub struct InsurancePolicy {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub external_ref: Option<String>,
    pub coverage_type: String,
    pub coverage_type: CoverageType,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub active: bool,
    pub next_payment_date: u64,
    pub schedule_id: Option<u32>,
    pub tags: Vec<String>,
}


/// Paginated result for insurance policy queries
#[contracttype]
#[derive(Clone)]
pub struct PolicyPage {
    /// Policies for this page
    pub items: Vec<InsurancePolicy>,
    /// Pass as `cursor` for the next page. 0 = no more pages.
    pub next_cursor: u32,
    /// Number of items returned
    pub count: u32,
}

/// Schedule for automatic premium payments
#[contracttype]
#[derive(Clone)]
pub struct PremiumSchedule {
    pub id: u32,
    pub owner: Address,
    pub policy_id: u32,
    pub next_due: u64,
    pub interval: u64,
    pub recurring: bool,
    pub active: bool,
    pub created_at: u64,
    pub last_executed: Option<u64>,
    pub missed_count: u32,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum InsuranceError {
    InvalidPremium = 1,
    InvalidCoverage = 2,
    PolicyNotFound = 3,
    PolicyInactive = 4,
    Unauthorized = 5,
    BatchTooLarge = 6,
}



#[contracttype]
#[derive(Clone)]
pub enum InsuranceEvent {
    PolicyCreated,
    PremiumPaid,
    PolicyDeactivated,
    ExternalRefUpdated,
    ScheduleCreated,
    ScheduleExecuted,
    ScheduleMissed,
    ScheduleModified,
    ScheduleCancelled,
}

#[contract]
pub struct Insurance;

#[contractimpl]
impl Insurance {
    /// Create a new insurance policy
    ///
    /// # Arguments
    /// * `owner` - Address of the policy owner (must authorize)
    /// * `name` - Name of the policy
    /// * `coverage_type` - Type of coverage (e.g., "health", "emergency")
    /// * `monthly_premium` - Monthly premium amount (must be positive)
    /// * `coverage_amount` - Total coverage amount (must be positive)
    /// * `external_ref` - Optional external system reference ID
    ///
    /// # Returns
    /// The ID of the created policy
    ///
    /// # Panics
    /// - If owner doesn't authorize the transaction
    /// - If monthly_premium is not positive
    /// - If coverage_amount is not positive
    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn clamp_limit(limit: u32) -> u32 {
        if limit == 0 {
            DEFAULT_PAGE_LIMIT
        } else if limit > MAX_PAGE_LIMIT {
            MAX_PAGE_LIMIT
        } else {
            limit
        }
    }

    fn get_pause_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("PAUSE_ADM"))
    }
    fn get_global_paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&symbol_short!("PAUSED"))
            .unwrap_or(false)
    }
    fn is_function_paused(env: &Env, func: Symbol) -> bool {
        env.storage()
            .instance()
            .get::<_, Map<Symbol, bool>>(&symbol_short!("PAUSED_FN"))
            .unwrap_or_else(|| Map::new(env))
            .get(func)
            .unwrap_or(false)
    }
    fn require_not_paused(env: &Env, func: Symbol) -> Result<(), InsuranceError> {
        if Self::get_global_paused(env) {
            return Err(InsuranceError::ContractPaused);
        }
        if Self::is_function_paused(env, func) {
            return Err(InsuranceError::FunctionPaused);
        }
        Ok(())
    }

    pub fn set_pause_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), InsuranceError> {
        caller.require_auth();
        let current = Self::get_pause_admin(&env);
        match current {
            None => {
                if caller != new_admin {
                    return Err(InsuranceError::Unauthorized);
                }
            }
            Some(admin) if admin != caller => return Err(InsuranceError::Unauthorized),
            _ => {}
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSE_ADM"), &new_admin);
        Ok(())
    }
    pub fn pause(env: Env, caller: Address) -> Result<(), InsuranceError> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).unwrap_or_else(|| panic!("No pause admin set"));
        let admin = Self::get_pause_admin(&env).ok_or(InsuranceError::Unauthorized)?;
        if admin != caller {
            return Err(InsuranceError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &true);
        env.events()
            .publish((symbol_short!("insure"), symbol_short!("paused")), ());
        Ok(())
    }
    pub fn unpause(env: Env, caller: Address) -> Result<(), InsuranceError> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).unwrap_or_else(|| panic!("No pause admin set"));
        let admin = Self::get_pause_admin(&env).ok_or(InsuranceError::Unauthorized)?;
        if admin != caller {
            return Err(InsuranceError::Unauthorized);
        }
        let unpause_at: Option<u64> = env.storage().instance().get(&symbol_short!("UNP_AT"));
        if let Some(at) = unpause_at {
            if env.ledger().timestamp() < at {
                panic!("Time-locked unpause not yet reached");
            }
            env.storage().instance().remove(&symbol_short!("UNP_AT"));
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &false);
        env.events()
            .publish((symbol_short!("insure"), symbol_short!("unpaused")), ());
        Ok(())
    }
    pub fn pause_function(env: Env, caller: Address, func: Symbol) -> Result<(), InsuranceError> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).unwrap_or_else(|| panic!("No pause admin set"));
        if admin != caller {
            return Err(InsuranceError::Unauthorized);
        }
        let mut m: Map<Symbol, bool> = env
            .storage()
            .instance()
            .get(&symbol_short!("PAUSED_FN"))
            .unwrap_or_else(|| Map::new(&env));
        m.set(func, true);
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED_FN"), &m);
        Ok(())
    }
    pub fn unpause_function(env: Env, caller: Address, func: Symbol) -> Result<(), InsuranceError> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).unwrap_or_else(|| panic!("No pause admin set"));
        if admin != caller {
            return Err(InsuranceError::Unauthorized);
        }
        let mut m: Map<Symbol, bool> = env
            .storage()
            .instance()
            .get(&symbol_short!("PAUSED_FN"))
            .unwrap_or_else(|| Map::new(&env));
        m.set(func, false);
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED_FN"), &m);
        Ok(())
    }
    pub fn emergency_pause_all(env: Env, caller: Address) {
        let _ = Self::pause(env.clone(), caller.clone());
        for func in [
            pause_functions::CREATE_POLICY,
            pause_functions::PAY_PREMIUM,
            pause_functions::DEACTIVATE,
            pause_functions::CREATE_SCHED,
            pause_functions::MODIFY_SCHED,
            pause_functions::CANCEL_SCHED,
        ] {
            let _ = Self::pause_function(env.clone(), caller.clone(), func);
        }
    }
    pub fn is_paused(env: Env) -> bool {
        Self::get_global_paused(&env)
    }
    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("VERSION"))
            .unwrap_or(CONTRACT_VERSION)
    }
    fn get_upgrade_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("UPG_ADM"))
    }
    pub fn set_upgrade_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), InsuranceError> {
        caller.require_auth();
        let current = Self::get_upgrade_admin(&env);
        match current {
            None => {
                if caller != new_admin {
                    return Err(InsuranceError::Unauthorized);
                }
            }
            Some(adm) if adm != caller => return Err(InsuranceError::Unauthorized),
            _ => {}
        }
        env.storage()
            .instance()
            .set(&symbol_short!("UPG_ADM"), &new_admin);
        Ok(())
    }
    pub fn set_version(env: Env, caller: Address, new_version: u32) -> Result<(), InsuranceError> {
        caller.require_auth();
        let admin = match Self::get_upgrade_admin(&env) {
            Some(a) => a,
            None => panic!("No upgrade admin set"),
        };
        if admin != caller {
            return Err(InsuranceError::Unauthorized);
        }
        let prev = Self::get_version(env.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("VERSION"), &new_version);
        env.events().publish(
            (symbol_short!("insure"), symbol_short!("upgraded")),
            (prev, new_version),
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Tag management
    // -----------------------------------------------------------------------

    fn validate_tags(tags: &Vec<String>) {
        if tags.is_empty() {
            panic!("Tags cannot be empty");
        }
        for tag in tags.iter() {
            if tag.len() == 0 || tag.len() > 32 {
                panic!("Tag must be between 1 and 32 characters");
            }
        }
    }

    pub fn add_tags_to_policy(
        env: Env,
        caller: Address,
        policy_id: u32,
        tags: Vec<String>,
    ) {
        caller.require_auth();
        Self::validate_tags(&tags);
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).expect("Policy not found");

        if policy.owner != caller {
            panic!("Only the policy owner can add tags");
        }

        for tag in tags.iter() {
            policy.tags.push_back(tag);
        }

        policies.set(policy_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        env.events().publish(
            (symbol_short!("insure"), symbol_short!("tags_add")),
            (policy_id, caller, tags),
        );
    }

    pub fn remove_tags_from_policy(
        env: Env,
        caller: Address,
        policy_id: u32,
        tags: Vec<String>,
    ) {
        caller.require_auth();
        Self::validate_tags(&tags);
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).expect("Policy not found");

        if policy.owner != caller {
            panic!("Only the policy owner can remove tags");
        }

        let mut new_tags = Vec::new(&env);
        for existing_tag in policy.tags.iter() {
            let mut should_keep = true;
            for remove_tag in tags.iter() {
                if existing_tag == remove_tag {
                    should_keep = false;
                    break;
                }
            }
            if should_keep {
                new_tags.push_back(existing_tag);
            }
        }

        policy.tags = new_tags;
        policies.set(policy_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        env.events().publish(
            (symbol_short!("insure"), symbol_short!("tags_rem")),
            (policy_id, caller, tags),
        );
    }

    // -----------------------------------------------------------------------
    // Tag management
    // -----------------------------------------------------------------------

    fn validate_tags(tags: &Vec<String>) {
        if tags.is_empty() {
            panic!("Tags cannot be empty");
        }
        for tag in tags.iter() {
            if tag.len() == 0 || tag.len() > 32 {
                panic!("Tag must be between 1 and 32 characters");
            }
        }
    }

    pub fn add_tags_to_policy(
        env: Env,
        caller: Address,
        policy_id: u32,
        tags: Vec<String>,
    ) {
        caller.require_auth();
        Self::validate_tags(&tags);
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).expect("Policy not found");

        if policy.owner != caller {
            panic!("Only the policy owner can add tags");
        }

        for tag in tags.iter() {
            policy.tags.push_back(tag);
        }

        policies.set(policy_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        env.events().publish(
            (symbol_short!("insure"), symbol_short!("tags_add")),
            (policy_id, caller, tags),
        );
    }

    pub fn remove_tags_from_policy(
        env: Env,
        caller: Address,
        policy_id: u32,
        tags: Vec<String>,
    ) {
        caller.require_auth();
        Self::validate_tags(&tags);
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).expect("Policy not found");

        if policy.owner != caller {
            panic!("Only the policy owner can remove tags");
        }

        let mut new_tags = Vec::new(&env);
        for existing_tag in policy.tags.iter() {
            let mut should_keep = true;
            for remove_tag in tags.iter() {
                if existing_tag == remove_tag {
                    should_keep = false;
                    break;
                }
            }
            if should_keep {
                new_tags.push_back(existing_tag);
            }
        }

        policy.tags = new_tags;
        policies.set(policy_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        env.events().publish(
            (symbol_short!("insure"), symbol_short!("tags_rem")),
            (policy_id, caller, tags),
        );
    }

    // -----------------------------------------------------------------------
    // Tag management
    // -----------------------------------------------------------------------

    fn validate_tags(tags: &Vec<String>) {
        if tags.is_empty() {
            panic!("Tags cannot be empty");
        }
        for tag in tags.iter() {
            if tag.len() == 0 || tag.len() > 32 {
                panic!("Tag must be between 1 and 32 characters");
            }
        }
    }

    pub fn add_tags_to_policy(
        env: Env,
        caller: Address,
        policy_id: u32,
        tags: Vec<String>,
    ) {
        caller.require_auth();
        Self::validate_tags(&tags);
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).expect("Policy not found");

        if policy.owner != caller {
            panic!("Only the policy owner can add tags");
        }

        for tag in tags.iter() {
            policy.tags.push_back(tag);
        }

        policies.set(policy_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        env.events().publish(
            (symbol_short!("insure"), symbol_short!("tags_add")),
            (policy_id, caller, tags),
        );
    }

    pub fn remove_tags_from_policy(
        env: Env,
        caller: Address,
        policy_id: u32,
        tags: Vec<String>,
    ) {
        caller.require_auth();
        Self::validate_tags(&tags);
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).expect("Policy not found");

        if policy.owner != caller {
            panic!("Only the policy owner can remove tags");
        }

        let mut new_tags = Vec::new(&env);
        for existing_tag in policy.tags.iter() {
            let mut should_keep = true;
            for remove_tag in tags.iter() {
                if existing_tag == remove_tag {
                    should_keep = false;
                    break;
                }
            }
            if should_keep {
                new_tags.push_back(existing_tag);
            }
        }

        policy.tags = new_tags;
        policies.set(policy_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        env.events().publish(
            (symbol_short!("insure"), symbol_short!("tags_rem")),
            (policy_id, caller, tags),
        );
    }

    // -----------------------------------------------------------------------
    // Core policy operations (unchanged)
    // -----------------------------------------------------------------------

    /// Creates a new insurance policy for the owner.
    ///
    /// # Arguments
    /// * `owner` - Address of the policy owner (must authorize)
    /// * `name` - Policy name (e.g., "Life Insurance")
    /// * `coverage_type` - Type of coverage (e.g., "Term", "Whole")
    /// * `monthly_premium` - Monthly premium amount in stroops (must be > 0)
    /// * `coverage_amount` - Total coverage amount in stroops (must be > 0)
    ///
    /// # Returns
    /// `Ok(policy_id)` - The newly created policy ID
    ///
    /// # Errors
    /// * `InvalidAmount` - If monthly_premium ≤ 0 or coverage_amount ≤ 0
    ///
    /// # Panics
    /// * If `owner` does not authorize the transaction (implicit via `require_auth()`)
    /// * If the contract is globally or function-specifically paused
    pub fn create_policy(
        env: Env,
        owner: Address,
        name: String,
        coverage_type: CoverageType,
        monthly_premium: i128,
        coverage_amount: i128,
        external_ref: Option<String>,
    ) -> u32 {
    ) -> Result<u32, InsuranceError> {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::CREATE_POLICY)?;

        if monthly_premium <= 0 || coverage_amount <= 0 {
            return Err(InsuranceError::InvalidAmount);
        }

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

        let next_payment_date = env.ledger().timestamp() + (30 * 86400);

        let policy = InsurancePolicy {
            id: next_id,
            owner: owner.clone(),
            name: name.clone(),
            external_ref,
            coverage_type: coverage_type.clone(),
            monthly_premium,
            coverage_amount,
            active: true,
            next_payment_date,
            schedule_id: None,
            tags: Vec::new(&env),
        };

        let policy_owner = policy.owner.clone();
        let policy_external_ref = policy.external_ref.clone();
        policies.set(next_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);
        Self::adjust_active_premium_total(&env, &owner, monthly_premium);

        env.events().publish(
            (POLICY_CREATED,),
            PolicyCreatedEvent {
                policy_id: next_id,
                name,
                coverage_type,
                monthly_premium,
                coverage_amount,
                timestamp: env.ledger().timestamp(),
            },
        );

        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PolicyCreated),
            (next_id, policy_owner, policy_external_ref),
            (next_id, owner),
        );

        Ok(next_id)
    }

    /// Pays a premium for a specific policy.
    ///
    /// # Arguments
    /// * `caller` - Address of the policy owner (must authorize)
    /// * `policy_id` - ID of the policy to pay premium for
    ///
    /// # Returns
    /// `Ok(())` on successful premium payment
    ///
    /// # Errors
    /// * `PolicyNotFound` - If policy_id does not exist
    /// * `Unauthorized` - If caller is not the policy owner
    /// * `PolicyInactive` - If the policy is not active
    ///
    /// # Panics
    /// * If `caller` does not authorize the transaction
    pub fn pay_premium(env: Env, caller: Address, policy_id: u32) -> Result<(), InsuranceError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::PAY_PREMIUM)?;
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = match policies.get(policy_id) {
            Some(p) => p,
            None => return Err(InsuranceError::PolicyNotFound),
        };

        if policy.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }
        if !policy.active {
            return Err(InsuranceError::PolicyInactive);
        }

        policy.next_payment_date = env.ledger().timestamp() + (30 * 86400);

        let policy_external_ref = policy.external_ref.clone();
        let event = PremiumPaidEvent {
            policy_id,
            name: policy.name.clone(),
            amount: policy.monthly_premium,
            next_payment_date: policy.next_payment_date,
            timestamp: env.ledger().timestamp(),
        };
        env.events().publish((PREMIUM_PAID,), event);

        policies.set(policy_id, policy);
        policies.set(policy_id, policy.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        env.events().publish(
            (PREMIUM_PAID,),
            PremiumPaidEvent {
                policy_id,
                name: policy.name,
                amount: policy.monthly_premium,
                next_payment_date: policy.next_payment_date,
                timestamp: env.ledger().timestamp(),
            },
        );

        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PremiumPaid),
            (policy_id, caller, policy_external_ref),
        );

        Ok(())
    }

    pub fn batch_pay_premiums(
        env: Env,
        caller: Address,
        policy_ids: Vec<u32>,
    ) -> Result<u32, InsuranceError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::PAY_PREMIUM)?;
        if policy_ids.len() > MAX_BATCH_SIZE {
            return Err(InsuranceError::BatchTooLarge);
        }
        let mut policies_map: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));
        for id in policy_ids.iter() {
            let policy = match policies_map.get(id) {
                Some(p) => p,
                None => return Err(InsuranceError::PolicyNotFound),
            };
            if policy.owner != caller {
                return Err(InsuranceError::Unauthorized);
            }
            if !policy.active {
                return Err(InsuranceError::PolicyInactive);
            }
        }

        let current_time = env.ledger().timestamp();
        let mut paid_count = 0;
        for id in policy_ids.iter() {
            let mut policy = policies.get(id).unwrap_or_else(|| panic!("Policy not found"));
            let mut policy = policies_map.get(id).unwrap();
            policy.next_payment_date = current_time + (30 * 86400);
            let event = PremiumPaidEvent {
                policy_id: id,
                name: policy.name.clone(),
                amount: policy.monthly_premium,
                next_payment_date: policy.next_payment_date,
                timestamp: current_time,
            };
            env.events().publish((PREMIUM_PAID,), event);
            env.events().publish(
                (symbol_short!("insure"), InsuranceEvent::PremiumPaid),
                (id, caller.clone()),
            );
            policies_map.set(id, policy);
            paid_count += 1;
        }
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies_map);
        env.events().publish(
            (symbol_short!("insure"), symbol_short!("batch_pay")),
            (paid_count, caller),
        );
        Ok(paid_count)
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
        for (_, policy) in policies.iter() {
            if policy.active && policy.owner == owner {
                result.push_back(policy);
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
        if let Some(totals) = Self::get_active_premium_totals_map(&env) {
            if let Some(total) = totals.get(owner.clone()) {
                return total;
            }
        }

        let mut total = 0i128;
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        for (_, policy) in policies.iter() {
            if policy.active && policy.owner == owner {
                total += policy.monthly_premium;
            }
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
    pub fn deactivate_policy(
        env: Env,
        caller: Address,
        policy_id: u32,
    ) -> Result<bool, InsuranceError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::DEACTIVATE)?;

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).unwrap_or_else(|| panic!("Policy not found"));
        let mut policy = policies
            .get(policy_id)
            .ok_or(InsuranceError::PolicyNotFound)?;

        if policy.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }

        let was_active = policy.active;
        policy.active = false;
        let policy_external_ref = policy.external_ref.clone();
        policies.set(policy_id, policy);
        let premium_amount = policy.monthly_premium;
        policies.set(policy_id, policy.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        if was_active {
            Self::adjust_active_premium_total(&env, &caller, -premium_amount);
        }
        let event = PolicyDeactivatedEvent {
            policy_id,
            name: policy.name.clone(),
            timestamp: env.ledger().timestamp(),
        };
        env.events().publish((POLICY_DEACTIVATED,), event);
        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PolicyDeactivated),
            (policy_id, caller, policy_external_ref),
        );

        true
    }

    /// Set or clear an external reference ID for a policy
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the policy owner)
    /// * `policy_id` - ID of the policy
    /// * `external_ref` - Optional external system reference ID
    ///
    /// # Returns
    /// True if the reference update was successful
    ///
    /// # Panics
    /// - If caller is not the policy owner
    /// - If policy is not found
    pub fn set_external_ref(
        env: Env,
        caller: Address,
        policy_id: u32,
        external_ref: Option<String>,
    ) -> bool {
        caller.require_auth();

        Self::extend_instance_ttl(&env);
        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).expect("Policy not found");
        if policy.owner != caller {
            panic!("Only the policy owner can update this policy reference");
        }

        policy.external_ref = external_ref.clone();
        policies.set(policy_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::ExternalRefUpdated),
            (policy_id, caller, external_ref),
            (symbol_short!("insuranc"), InsuranceEvent::PolicyDeactivated),
            (policy_id, caller),
        );

        Ok(true)
    }

    /// Extend the TTL of instance storage
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn get_active_premium_totals_map(env: &Env) -> Option<Map<Address, i128>> {
        env.storage().instance().get(&STORAGE_PREMIUM_TOTALS)
    }

    fn adjust_active_premium_total(env: &Env, owner: &Address, delta: i128) {
        if delta == 0 {
            return;
        }
        let mut totals: Map<Address, i128> = env
            .storage()
            .instance()
            .get(&STORAGE_PREMIUM_TOTALS)
            .unwrap_or_else(|| Map::new(env));
        let current = totals.get(owner.clone()).unwrap_or(0);
        let next = if delta >= 0 {
            current.saturating_add(delta)
        } else {
            current.saturating_sub(delta.saturating_abs())
        };
        totals.set(owner.clone(), next);
        env.storage()
            .instance()
            .set(&STORAGE_PREMIUM_TOTALS, &totals);
    }

    // -----------------------------------------------------------------------
    // Schedule operations (unchanged)
    // -----------------------------------------------------------------------
    pub fn create_premium_schedule(
        env: Env,
        owner: Address,
        policy_id: u32,
        next_due: u64,
        interval: u64,
    ) -> Result<u32, InsuranceError> {
        // Changed to Result
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::CREATE_SCHED)?;

        let name = String::from_str(&env, "Health Insurance");
        let coverage_type = String::from_str(&env, "health");
        let monthly_premium = 100;
        let coverage_amount = 10000;
        let external_ref = Some(String::from_str(&env, "POLICY-EXT-1"));

        let policy_id = client.create_policy(
            &owner,
            &name,
            &coverage_type,
            &monthly_premium,
            &coverage_amount,
            &external_ref,
        );
        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).unwrap_or_else(|| panic!("Policy not found"));
        let mut policy = policies
            .get(policy_id)
            .ok_or(InsuranceError::PolicyNotFound)?;

        if policy.owner != owner {
            return Err(InsuranceError::Unauthorized);
        }

        let policy = client.get_policy(&policy_id).unwrap();
        assert_eq!(policy.id, 1);
        assert_eq!(policy.owner, owner);
        assert_eq!(policy.name, name);
        assert_eq!(policy.external_ref, external_ref);
        assert_eq!(policy.coverage_type, coverage_type);
        assert_eq!(policy.monthly_premium, monthly_premium);
        assert_eq!(policy.coverage_amount, coverage_amount);
        assert!(policy.active);
        assert_eq!(policy.next_payment_date, 1000000000 + (30 * 86400));
    }
        let current_time = env.ledger().timestamp();
        if next_due <= current_time {
            return Err(InsuranceError::InvalidTimestamp);
        }

        Self::extend_instance_ttl(&env);

        let mut schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("PREM_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        client.create_policy(&owner, &name, &coverage_type, &0, &10000, &None);
    }
        let next_schedule_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_PSCH"))
            .unwrap_or(0u32)
            + 1;

        let schedule = PremiumSchedule {
            id: next_schedule_id,
            owner: owner.clone(),
            policy_id,
            next_due,
            interval,
            recurring: interval > 0,
            active: true,
            created_at: current_time,
            last_executed: None,
            missed_count: 0,
        };

        policy.schedule_id = Some(next_schedule_id);

        client.create_policy(&owner, &name, &coverage_type, &-100, &10000, &None);
    }
        schedules.set(next_schedule_id, schedule);
        env.storage()
            .instance()
            .set(&symbol_short!("PREM_SCH"), &schedules);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_PSCH"), &next_schedule_id);

        policies.set(policy_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::ScheduleCreated),
            (next_schedule_id, owner),
        );

        Ok(next_schedule_id)
    }

    /// Modify a premium schedule
    pub fn modify_premium_schedule(
        env: Env,
        caller: Address,
        schedule_id: u32,
        next_due: u64,
        interval: u64,
    ) -> Result<bool, InsuranceError> {
        // Changed to Result
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::MODIFY_SCHED)?;

        let current_time = env.ledger().timestamp();
        if next_due <= current_time {
            return Err(InsuranceError::InvalidTimestamp); // Use Err instead of panic
        }

        Self::extend_instance_ttl(&env);

        let mut schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("PREM_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        let mut schedule = schedules.get(schedule_id).unwrap_or_else(|| panic!("Schedule not found"));
        let mut schedule = schedules
            .get(schedule_id)
            .ok_or(InsuranceError::PolicyNotFound)?;

        if schedule.owner != caller {
            return Err(InsuranceError::Unauthorized); // Use Err instead of panic
        }

        schedule.next_due = next_due;
        schedule.interval = interval;
        schedule.recurring = interval > 0;

        schedules.set(schedule_id, schedule);
        env.storage()
            .instance()
            .set(&symbol_short!("PREM_SCH"), &schedules);

        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::ScheduleModified),
            (schedule_id, caller),
        );

        Ok(true) // Wrap return value in Ok
    }

    /// Cancel a premium schedule
    pub fn cancel_premium_schedule(
        env: Env,
        caller: Address,
        schedule_id: u32,
    ) -> Result<bool, InsuranceError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::CANCEL_SCHED)?;

        Self::extend_instance_ttl(&env);

        let mut schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("PREM_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        let mut schedule = schedules.get(schedule_id).unwrap_or_else(|| panic!("Schedule not found"));
        let mut schedule = schedules
            .get(schedule_id)
            .ok_or(InsuranceError::PolicyNotFound)?;

        if schedule.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }

        schedule.active = false;

        schedules.set(schedule_id, schedule);
        env.storage()
            .instance()
            .set(&symbol_short!("PREM_SCH"), &schedules);

        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::ScheduleCancelled),
            (schedule_id, caller),
        );

        Ok(true)
    }

    /// Execute due premium schedules (public, callable by anyone - keeper pattern)
    pub fn execute_due_premium_schedules(env: Env) -> Vec<u32> {
        Self::extend_instance_ttl(&env);

        let current_time = env.ledger().timestamp();
        let mut executed = Vec::new(&env);

        let mut schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("PREM_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        for (schedule_id, mut schedule) in schedules.iter() {
            if !schedule.active || schedule.next_due > current_time {
                continue;
            }

            if let Some(mut policy) = policies.get(schedule.policy_id) {
                if policy.active {
                    policy.next_payment_date = current_time + (30 * 86400);
                    policies.set(schedule.policy_id, policy.clone());

                    env.events().publish(
                        (symbol_short!("insure"), InsuranceEvent::PremiumPaid),
                        (schedule.policy_id, policy.owner),
                    );
                }
            }

            schedule.last_executed = Some(current_time);

            if schedule.recurring && schedule.interval > 0 {
                let mut missed = 0u32;
                let mut next = schedule.next_due + schedule.interval;
                while next <= current_time {
                    missed += 1;
                    next += schedule.interval;
                }
                schedule.missed_count += missed;
                schedule.next_due = next;

                if missed > 0 {
                    env.events().publish(
                        (symbol_short!("insure"), InsuranceEvent::ScheduleMissed),
                        (schedule_id, missed),
                    );
                }
            } else {
                schedule.active = false;
            }

            schedules.set(schedule_id, schedule);
            executed.push_back(schedule_id);

            env.events().publish(
                (symbol_short!("insure"), InsuranceEvent::ScheduleExecuted),
                schedule_id,
            );
        }

        env.storage()
            .instance()
            .set(&symbol_short!("PREM_SCH"), &schedules);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        executed
    }

    /// Get all premium schedules for an owner
    pub fn get_premium_schedules(env: Env, owner: Address) -> Vec<PremiumSchedule> {
        let schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("PREM_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        for (_, schedule) in schedules.iter() {
            if schedule.owner == owner {
                result.push_back(schedule);
            }
        }
        result
    }

    /// Get a specific premium schedule
    pub fn get_premium_schedule(env: Env, schedule_id: u32) -> Option<PremiumSchedule> {
        let schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("PREM_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        schedules.get(schedule_id)
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_events {
    use super::*;
    use proptest::prelude::*;
    use soroban_sdk::testutils::storage::Instance as _;
    use soroban_sdk::testutils::{Address as _, Events, Ledger, LedgerInfo};
    use soroban_sdk::{Env, String};

    fn make_env() -> Env {
        Env::default()
    }

    fn setup_policies(
        env: &Env,
        client: &InsuranceClient,
        owner: &Address,
        count: u32,
    ) -> Vec<u32> {
        let mut ids = Vec::new(env);
        for i in 0..count {
            let id = client.create_policy(
                owner,
                &String::from_str(env, "Policy"),
                &CoverageType::Health,
                &(50i128 * (i as i128 + 1)),
                &(10000i128 * (i as i128 + 1)),
            );
            ids.push_back(id);
        }
        ids
    }

    // --- get_active_policies ---

    #[test]
    fn test_create_policy_invalid_premium() {
        let env = Env::default();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let page = client.get_active_policies(&owner, &0, &0);
        assert_eq!(page.count, 0);
        assert_eq!(page.next_cursor, 0);
    }

        client.create_policy(&owner, &name, &coverage_type, &100, &0, &None);
    #[test]
    fn test_get_active_policies_single_page() {
        let env = make_env();
        env.mock_all_auths();

        // Use the .try_ version of the function to capture the error result
        let result = client.try_create_policy(
            &owner,
            &String::from_str(&env, "Life"),
            &String::from_str(&env, "Health"),
            &0, // This is invalid
            &10000,
        );

        // Assert that the result matches our custom error code
        assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));
    }

    #[test]
    fn test_create_policy_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // No policies created — policy ID 999 does not exist; contract panics
        let result = client.try_pay_premium(&owner, &999u32);
        // Create a policy
        let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "Health Insurance"),
            &String::from_str(&env, "health"),
            &100,
            &50000,
        );
        assert_eq!(policy_id, 1);

        // Contract panics when policy not found
        assert!(result.is_err());
    }

    #[test]
    fn test_get_active_policies_pagination() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let name = String::from_str(&env, "Health Insurance");
        let coverage_type = String::from_str(&env, "health");
        let policy_id = client.create_policy(&owner, &name, &coverage_type, &100, &10000, &None);
        setup_policies(&env, &client, &owner, 7);

        let page1 = client.get_active_policies(&owner, &0, &3);
        assert_eq!(page1.count, 3);
        assert!(page1.next_cursor > 0);

        let page2 = client.get_active_policies(&owner, &page1.next_cursor, &3);
        assert_eq!(page2.count, 3);
        assert!(page2.next_cursor > 0);

        let page3 = client.get_active_policies(&owner, &page2.next_cursor, &3);
        assert_eq!(page3.count, 1);
        assert_eq!(page3.next_cursor, 0);
    }
        // Create a policy
        let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "Emergency Coverage"),
            &String::from_str(&env, "emergency"),
            &75,
            &25000,
        );

        env.mock_all_auths();

        let name = String::from_str(&env, "Health Insurance");
        let coverage_type = String::from_str(&env, "health");
        let policy_id = client.create_policy(&owner, &name, &coverage_type, &100, &10000, &None);
        let ids = setup_policies(&env, &client, &owner, 4);
        // Deactivate policy #2
        client.deactivate_policy(&owner, &ids.get(1).unwrap());

        let page = client.get_active_policies(&owner, &0, &10);
        assert_eq!(page.count, 3); // only 3 active
        for p in page.items.iter() {
            assert!(p.active, "only active policies should be returned");
        }
    }

    #[test]
    fn test_get_active_policies_multi_owner_isolation() {
        let env = make_env();
        env.mock_all_auths();
        let id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &id);
        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);
        // Get events before paying premium
        let events_before = env.events().all().len();

        // Pay premium
        let result = client.pay_premium(&owner, &policy_id);
        assert!(result);

        // Verify PremiumPaid event was emitted (2 new events: topic + enum)
        let events_after = env.events().all().len();
        assert_eq!(events_after - events_before, 2);
    }

    #[test]
    fn test_deactivate_policy_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Create a policy
        let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "Life Insurance"),
            &String::from_str(&env, "life"),
            &200,
            &100000,
        );

        env.mock_all_auths();

        // Get events before deactivating
        let events_before = env.events().all().len();

        // Deactivate policy
        let result = client.deactivate_policy(&owner, &policy_id);
        assert!(result);

        // Verify PolicyDeactivated event was emitted (2 new events: topic + enum)
        let events_after = env.events().all().len();
        assert_eq!(events_after - events_before, 2);
    }

    #[test]
    fn test_create_policy_emits_event_exists() {
        let env = make_env();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Create multiple policies
        let name1 = String::from_str(&env, "Health Insurance");
        let coverage_type1 = String::from_str(&env, "health");
        let policy_id1 = client.create_policy(&owner, &name1, &coverage_type1, &100, &10000, &None);

        let name2 = String::from_str(&env, "Emergency Insurance");
        let coverage_type2 = String::from_str(&env, "emergency");
        let policy_id2 = client.create_policy(&owner, &name2, &coverage_type2, &200, &20000, &None);

        let name3 = String::from_str(&env, "Life Insurance");
        let coverage_type3 = String::from_str(&env, "life");
        let policy_id3 = client.create_policy(&owner, &name3, &coverage_type3, &300, &30000, &None);
        let policy_id = client.create_policy(
        client.create_policy(
            &owner,
            &String::from_str(&env, "Health Insurance"),
            &CoverageType::Health,
            &String::from_str(&env, "Policy 1"),
            &String::from_str(&env, "health"),
            &100,
            &50000,
        );
        client.create_policy(
            &owner,
            &String::from_str(&env, "Policy 2"),
            &String::from_str(&env, "life"),
            &200,
            &100000,
        );
        client.create_policy(
            &owner,
            &String::from_str(&env, "Policy 3"),
            &String::from_str(&env, "emergency"),
            &75,
            &25000,
        );

        client.pay_premium(&owner, &policy_id);

        let events_after = env.events().all().len();
        assert_eq!(events_after - events_before, 2);
    }

    #[test]
    fn test_policy_lifecycle_emits_all_events() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Create multiple policies
        let name1 = String::from_str(&env, "Health Insurance");
        let coverage_type1 = String::from_str(&env, "health");
        client.create_policy(&owner, &name1, &coverage_type1, &100, &10000, &None);

        let name2 = String::from_str(&env, "Emergency Insurance");
        let coverage_type2 = String::from_str(&env, "emergency");
        client.create_policy(&owner, &name2, &coverage_type2, &200, &20000, &None);

        let name3 = String::from_str(&env, "Life Insurance");
        let coverage_type3 = String::from_str(&env, "life");
        let policy_id3 = client.create_policy(&owner, &name3, &coverage_type3, &300, &30000, &None);
        // Create a policy
        let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "Complete Lifecycle"),
            &CoverageType::Health,
            &150,
            &75000,
        );

        env.mock_all_auths();

        // Pay premium
        client.pay_premium(&owner, &policy_id);

        // Deactivate
        client.deactivate_policy(&owner, &policy_id);

        // Should have 6 events: 2 Created + 2 PremiumPaid + 2 Deactivated
        let events = env.events().all();
        assert_eq!(events.len(), 6);
    }

    // ====================================================================
    // Storage TTL Extension Tests
    //
    // Verify that instance storage TTL is properly extended on
    // state-changing operations, preventing unexpected data expiration.
    //
    // Contract TTL configuration:
    //   INSTANCE_LIFETIME_THRESHOLD = 17,280 ledgers (~1 day)
    //   INSTANCE_BUMP_AMOUNT        = 518,400 ledgers (~30 days)
    //
    // Operations extending instance TTL:
    //   create_policy, pay_premium, batch_pay_premiums,
    //   deactivate_policy, create_premium_schedule,
    //   modify_premium_schedule, cancel_premium_schedule,
    //   execute_due_premium_schedules
    // ====================================================================

    /// Verify that create_policy extends instance storage TTL.
    #[test]
    fn test_instance_ttl_extended_on_create_policy() {
        let env = Env::default();
        env.mock_all_auths();

        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 100,
            timestamp: 1000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let name = String::from_str(&env, "Health Insurance");
        let coverage_type = String::from_str(&env, "health");
        let policy_id = client.create_policy(&owner, &name, &coverage_type, &100, &10000, &None);

        let result = client.deactivate_policy(&owner, &policy_id);
        assert!(result);
        // create_policy calls extend_instance_ttl
        let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "Health Insurance"),
            &CoverageType::Health,
            &100,
            &50000,
        );
        assert_eq!(policy_id, 1);

        // Inspect instance TTL — must be at least INSTANCE_BUMP_AMOUNT
        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert!(
            ttl >= 518_400,
            "Instance TTL ({}) must be >= INSTANCE_BUMP_AMOUNT (518,400) after create_policy",
            ttl
        );
    }

    /// Verify that pay_premium refreshes instance TTL after ledger advancement.
    ///
    /// extend_ttl(threshold, extend_to) only extends when TTL <= threshold.
    /// We advance the ledger far enough for TTL to drop below 17,280.
    #[test]
    fn test_instance_ttl_refreshed_on_pay_premium() {
        let env = Env::default();
        env.mock_all_auths();

        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 100,
            timestamp: 1000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.create_policy(
            &owner,
            &String::from_str(&env, "Life Insurance"),
            &String::from_str(&env, "life"),
            &200,
            &100000,
        );

        // Advance ledger so TTL drops below threshold (17,280)
        // After create_policy: live_until = 518,500. At seq 510,000: TTL = 8,500
        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 510_000,
            timestamp: 500_000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        // pay_premium calls extend_instance_ttl → re-extends TTL to 518,400
        client.pay_premium(&owner, &1);

        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert!(
            ttl >= 518_400,
            "Instance TTL ({}) must be >= 518,400 after pay_premium",
            ttl
        );
    }

    /// Verify data persists across repeated operations spanning multiple
    /// ledger advancements, proving TTL is continuously renewed.
    #[test]
    fn test_set_external_ref_success() {
        let env = create_test_env();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let name = String::from_str(&env, "Health Insurance");
        let coverage_type = String::from_str(&env, "health");
        let policy_id = client.create_policy(&owner, &name, &coverage_type, &100, &10000, &None);

        let external_ref = Some(String::from_str(&env, "POLICY-EXT-99"));
        assert!(client.set_external_ref(&owner, &policy_id, &external_ref));

        let policy = client.get_policy(&policy_id).unwrap();
        assert_eq!(policy.external_ref, external_ref);
    }

    #[test]
    #[should_panic(expected = "Only the policy owner can update this policy reference")]
    fn test_set_external_ref_unauthorized() {
        let env = create_test_env();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let other = Address::generate(&env);

        let name = String::from_str(&env, "Health Insurance");
        let coverage_type = String::from_str(&env, "health");
        let policy_id = client.create_policy(&owner, &name, &coverage_type, &100, &10000, &None);

        client.set_external_ref(
            &other,
            &policy_id,
            &Some(String::from_str(&env, "POLICY-EXT-99")),
        );
    }

    #[test]
    fn test_multiple_policies_management() {
        let env = create_test_env();
    fn test_policy_data_persists_across_ledger_advancements() {
        let env = Env::default();
        env.mock_all_auths();

        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 100,
            timestamp: 1000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Phase 1: Create policy at seq 100. live_until = 518,500
        let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "Auto Insurance"),
            &String::from_str(&env, "auto"),
            &150,
            &75000,
        );

        for (i, policy_name) in policy_names.iter().enumerate() {
            let premium = ((i + 1) as i128) * 100;
            let coverage = ((i + 1) as i128) * 10000;
            let policy_id = client.create_policy(
                &owner,
                policy_name,
                &coverage_type,
                &premium,
                &coverage,
                &None,
            );
            policy_ids.push_back(policy_id);
        }
        // Phase 2: Advance to seq 510,000 (TTL = 8,500 < 17,280)
        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 510_000,
            timestamp: 510_000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        client.pay_premium(&owner, &policy_id);

        // Phase 3: Advance to seq 1,020,000 (TTL = 8,400 < 17,280)
        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 1_020_000,
            timestamp: 1_020_000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        let policy_id2 = client.create_policy(
            &owner,
            &String::from_str(&env, "Travel Insurance"),
            &String::from_str(&env, "travel"),
            &50,
            &20000,
        );

        // All policies should be accessible
        let p1 = client.get_policy(&policy_id);
        assert!(
            p1.is_some(),
            "First policy must persist across ledger advancements"
        );
        assert_eq!(p1.unwrap().monthly_premium, 150);

        let p2 = client.get_policy(&policy_id2);
        assert!(p2.is_some(), "Second policy must persist");

        // TTL should be fully refreshed
        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert!(
            ttl >= 518_400,
            "Instance TTL ({}) must remain >= 518,400 after repeated operations",
            ttl
        );
    }

    /// Verify that deactivate_policy extends instance TTL.
    #[test]
    fn test_instance_ttl_extended_on_deactivate_policy() {
        let env = Env::default();
        env.mock_all_auths();

        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 100,
            timestamp: 1000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "Dental"),
            &String::from_str(&env, "dental"),
            &75,
            &25000,
        );

        // Advance ledger past threshold
        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 510_000,
            timestamp: 510_000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        // deactivate_policy calls extend_instance_ttl
        client.deactivate_policy(&owner, &policy_id);

        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert!(
            ttl >= 518_400,
            "Instance TTL ({}) must be >= 518,400 after deactivate_policy",
            ttl
        );
    }

    // ──────────────────────────────────────────────────────────────────
    // Test: pay_premium after deactivate_policy (#104)
    // ──────────────────────────────────────────────────────────────────

    /// After deactivating a policy, `pay_premium` must return an error.
    /// The policy must remain inactive.
    #[test]
    fn test_pay_premium_after_deactivate() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // 1. Create a policy
        let policy_id = client.create_policy(
            &owner,
            &name,
            &coverage_type,
            &monthly_premium,
            &coverage_amount,
            &None,
            &String::from_str(&env, "Health Plan"),
            &CoverageType::Health,
            &150,
            &50000,
        );

        // Sanity: policy should be active after creation
        let policy_before = client.get_policy(&policy_id).unwrap();
        assert!(policy_before.active);

        // 2. Deactivate the policy
        let deactivated = client.deactivate_policy(&owner, &policy_id);
        assert!(deactivated);

        // Confirm it is now inactive
        let policy_after_deactivate = client.get_policy(&policy_id).unwrap();
        assert!(!policy_after_deactivate.active);

        // 3. Attempt to pay premium — should return PolicyInactive error
        let result = client.try_pay_premium(&owner, &policy_id);
        assert_eq!(result, Err(Ok(InsuranceError::PolicyInactive)));
    }

    // -----------------------------------------------------------------------
    // Property-based tests: time-dependent behavior
    // -----------------------------------------------------------------------

    proptest! {
        /// After paying a premium at any timestamp `now`,
        /// next_payment_date must always equal now + 30 days.
        #[test]
        fn prop_pay_premium_sets_next_payment_date(
            now in 1_000_000u64..100_000_000u64,
        ) {
            let env = make_env();
            env.ledger().set_timestamp(now);
            env.mock_all_auths();
            let cid = env.register_contract(None, Insurance);
            let client = InsuranceClient::new(&env, &cid);
            let owner = Address::generate(&env);

            let policy_id = client.create_policy(
                &owner,
                &String::from_str(&env, "Policy"),
                &String::from_str(&env, "health"),
                &100,
                &10000,
            );

            client.pay_premium(&owner, &policy_id);

            let policy = client.get_policy(&policy_id).unwrap();
            prop_assert_eq!(
                policy.next_payment_date,
                now + 30 * 86400,
                "next_payment_date must equal now + 30 days after premium payment"
            );
        }
    }

    proptest! {
        /// A premium schedule must not execute before its due date,
        /// and must execute at or after its due date.
        #[test]
        fn prop_execute_due_schedules_only_triggers_past_due(
            creation_time in 1_000_000u64..5_000_000u64,
            gap in 1000u64..1_000_000u64,
        ) {
            let env = make_env();
            env.ledger().set_timestamp(creation_time);
            env.mock_all_auths();
            let cid = env.register_contract(None, Insurance);
            let client = InsuranceClient::new(&env, &cid);
            let owner = Address::generate(&env);

            let policy_id = client.create_policy(
                &owner,
                &String::from_str(&env, "Policy"),
                &String::from_str(&env, "health"),
                &100,
                &10000,
            );

            // Schedule fires at creation_time + gap (strictly in the future)
            let next_due = creation_time + gap;
            let schedule_id = client.create_premium_schedule(&owner, &policy_id, &next_due, &0);

            // One tick before due: schedule must not execute
            env.ledger().set_timestamp(next_due - 1);
            let executed_before = client.execute_due_premium_schedules();
            prop_assert_eq!(
                executed_before.len(),
                0u32,
                "schedule must not fire before its due date"
            );

            // Exactly at due date: schedule must execute
            env.ledger().set_timestamp(next_due);
            let executed_at = client.execute_due_premium_schedules();
            prop_assert_eq!(executed_at.len(), 1u32);
            prop_assert_eq!(executed_at.get(0).unwrap(), schedule_id);
        }
    }

    // ══════════════════════════════════════════════════════════════════════
    // Time & Ledger Drift Resilience Tests (#158)
    //
    // Assumptions:
    //  - execute_due_premium_schedules fires when schedule.next_due <= current_time
    //    (inclusive: executes exactly at next_due).
    //  - next_payment_date = env.ledger().timestamp() + 30 * 86400 at execution,
    //    anchored to actual payment time, not original next_due.
    //  - Stellar ledger timestamps are monotonically increasing in production.
    //    After execution next_due advances by the interval, guarding re-runs.
    // ══════════════════════════════════════════════════════════════════════

    fn set_time(env: &Env, timestamp: u64) {
        let proto = env.ledger().protocol_version();
        env.ledger().set(LedgerInfo {
            protocol_version: proto,
            sequence_number: 1,
            timestamp,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: 100000,
        });
    }

    /// Premium schedule must NOT execute one second before next_due.
    #[test]
    fn test_time_drift_premium_schedule_not_executed_before_next_due() {
        let env = make_env();
        env.mock_all_auths();
        let id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &id);
        let owner = Address::generate(&env);

        let next_due = 5000u64;
        set_time(&env, 1000);

        let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "Life Cover"),
            &String::from_str(&env, "life"),
            &200,
            &100000,
        );
        client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);

        set_time(&env, next_due - 1);
        let executed = client.execute_due_premium_schedules();
        assert_eq!(
            executed.len(),
            0,
            "Must not execute one second before next_due"
        );
    }

    /// Premium schedule must execute exactly at next_due (inclusive boundary).
    #[test]
    fn test_time_drift_premium_schedule_executes_at_exact_next_due() {
        let env = make_env();
        env.mock_all_auths();
        let id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &id);
        let owner = Address::generate(&env);

        let next_due = 5000u64;
        set_time(&env, 1000);

        let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "Health Plan"),
            &String::from_str(&env, "health"),
            &150,
            &75000,
        );
        let schedule_id = client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);

        set_time(&env, next_due);
        let executed = client.execute_due_premium_schedules();
        assert_eq!(executed.len(), 1, "Must execute exactly at next_due");
        assert_eq!(executed.get(0).unwrap(), schedule_id);

        let policy = client.get_policy(&policy_id).unwrap();
        assert_eq!(
            policy.next_payment_date,
            next_due + 30 * 86400,
            "next_payment_date must be current_time + 30 days"
        );
    }

    /// next_payment_date is anchored to actual payment time, not original next_due.
    #[test]
    fn test_time_drift_next_payment_date_uses_actual_payment_time() {
        let env = make_env();
        env.mock_all_auths();
        let id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &id);
        let owner = Address::generate(&env);

        let next_due = 5000u64;
        let late_payment = next_due + 7 * 86400; // paid 7 days late
        set_time(&env, 1000);

        let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "Property Plan"),
            &String::from_str(&env, "property"),
            &300,
            &200000,
        );
        client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);

        set_time(&env, late_payment);
        client.execute_due_premium_schedules();

        let policy = client.get_policy(&policy_id).unwrap();
        assert_eq!(
            policy.next_payment_date,
            late_payment + 30 * 86400,
            "next_payment_date must be anchored to actual payment time"
        );
        assert!(
            policy.next_payment_date > next_due + 30 * 86400,
            "Late payment must push next_payment_date beyond on-time window"
        );
    }

    /// After execution next_due advances; a call before the new next_due must not re-execute.
    #[test]
    fn test_time_drift_no_double_execution_after_schedule_advances() {
        let env = make_env();
        env.mock_all_auths();
        let id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &id);
        let owner = Address::generate(&env);

        let next_due = 5000u64;
        let interval = 2_592_000u64;
        set_time(&env, 1000);

        let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "Auto Cover"),
            &String::from_str(&env, "auto"),
            &100,
            &50000,
        );
        client.create_premium_schedule(&owner, &policy_id, &next_due, &interval);

        // First execution at next_due
        set_time(&env, next_due);
        let executed = client.execute_due_premium_schedules();
        assert_eq!(executed.len(), 1);

        // Between old next_due and new next_due: no re-execution
        set_time(&env, next_due + 1000);
        let executed_again = client.execute_due_premium_schedules();
        assert_eq!(
            executed_again.len(),
            0,
            "Must not re-execute before the new next_due"
        );
    }

    // -----------------------------------------------------------------------
    // Property-based tests: time-dependent behavior
    // -----------------------------------------------------------------------

    proptest! {
        /// After paying a premium at any timestamp `now`,
        /// next_payment_date must always equal now + 30 days.
        #[test]
        fn prop_pay_premium_sets_next_payment_date(
            now in 1_000_000u64..100_000_000u64,
        ) {
            let env = make_env();
            env.ledger().set_timestamp(now);
            env.mock_all_auths();
            let cid = env.register_contract(None, Insurance);
            let client = InsuranceClient::new(&env, &cid);
            let owner = Address::generate(&env);

            let policy_id = client.create_policy(
                &owner,
                &String::from_str(&env, "Policy"),
                &String::from_str(&env, "health"),
                &100,
                &10000,
            );

            client.pay_premium(&owner, &policy_id);

            let policy = client.get_policy(&policy_id).unwrap();
            prop_assert_eq!(
                policy.next_payment_date,
                now + 30 * 86400,
                "next_payment_date must equal now + 30 days after premium payment"
            );
        }
    }

    proptest! {
        /// A premium schedule must not execute before its due date,
        /// and must execute at or after its due date.
        #[test]
        fn prop_execute_due_schedules_only_triggers_past_due(
            creation_time in 1_000_000u64..5_000_000u64,
            gap in 1000u64..1_000_000u64,
        ) {
            let env = make_env();
            env.ledger().set_timestamp(creation_time);
            env.mock_all_auths();
            let cid = env.register_contract(None, Insurance);
            let client = InsuranceClient::new(&env, &cid);
            let owner = Address::generate(&env);

            let policy_id = client.create_policy(
                &owner,
                &String::from_str(&env, "Policy"),
                &String::from_str(&env, "health"),
                &100,
                &10000,
            );

            // Schedule fires at creation_time + gap (strictly in the future)
            let next_due = creation_time + gap;
            let schedule_id = client.create_premium_schedule(&owner, &policy_id, &next_due, &0);

            // One tick before due: schedule must not execute
            env.ledger().set_timestamp(next_due - 1);
            let executed_before = client.execute_due_premium_schedules();
            prop_assert_eq!(
                executed_before.len(),
                0u32,
                "schedule must not fire before its due date"
            );

            // Exactly at due date: schedule must execute
            env.ledger().set_timestamp(next_due);
            let executed_at = client.execute_due_premium_schedules();
            prop_assert_eq!(executed_at.len(), 1u32);
            prop_assert_eq!(executed_at.get(0).unwrap(), schedule_id);
        }
    }
}
