#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, String,
    Symbol, Vec,
};

use remitwise_common::{CoverageType, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD};
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
    NotInitialized = 9,
    InvalidName = 10,
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
pub struct InsurancePolicy {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub external_ref: Option<String>,
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
    pub fn initialize(env: Env, admin: Address) -> Result<(), InsuranceError> {
        if Self::get_pause_admin(&env).is_some() {
            return Err(InsuranceError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSE_ADM"), &admin);
        Ok(())
    }

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
    fn require_initialized(env: &Env) -> Result<(), InsuranceError> {
        if Self::get_pause_admin(env).is_none() {
            panic!("not initialized");
        }
        Ok(())
    }

    fn require_not_paused(env: &Env, func: Symbol) -> Result<(), InsuranceError> {
        Self::require_initialized(env)?;
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
        let admin = Self::get_pause_admin(&env).ok_or(InsuranceError::Unauthorized)?;
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
        let admin = Self::get_pause_admin(&env).ok_or(InsuranceError::Unauthorized)?;
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
    ) -> Result<u32, InsuranceError> {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::CREATE_POLICY)?;

        if name.len() == 0 || name.len() > 64 {
            return Err(InsuranceError::InvalidName);
        }

        if let Some(ext_ref) = &external_ref {
            if ext_ref.len() > 128 {
                return Err(InsuranceError::InvalidName);
            }
        }

        if monthly_premium <= 0 || coverage_amount <= 0 {
            return Err(InsuranceError::InvalidAmount);
        }

        // Coverage type specific range checks (matching test expectations)
        match coverage_type {
            CoverageType::Health => {
                if monthly_premium < 100 { return Err(InsuranceError::InvalidAmount); }
            }
            CoverageType::Life => {
                if monthly_premium < 500 { return Err(InsuranceError::InvalidAmount); }
                if coverage_amount < 10000 { return Err(InsuranceError::InvalidAmount); }
            }
            CoverageType::Property => {
                if monthly_premium < 200 { return Err(InsuranceError::InvalidAmount); }
            }
            _ => {}
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

        policies.set(policy_id, policy.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

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
        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));
        for id in policy_ids.iter() {
            let policy = match policies.get(id) {
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
            let mut policy = policies.get(id).ok_or(InsuranceError::PolicyNotFound)?;
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
            policies.set(id, policy);
            paid_count += 1;
        }
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);
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
        Self::require_initialized(&env).unwrap();
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        policies.get(policy_id)
    }

    /// Get active policies for a specific owner with pagination
    ///
    /// # Arguments
    /// * `owner` - Address of the policy owner
    /// * `cursor` - Start after this policy ID (pass 0 for the first page)
    /// * `limit` - Maximum number of policies to return (clamped to MAX_PAGE_LIMIT)
    ///
    /// # Returns
    /// PolicyPage { items, next_cursor, count }
    pub fn get_active_policies(env: Env, owner: Address, cursor: u32, limit: u32) -> PolicyPage {
        Self::require_initialized(&env).unwrap();
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let limit = Self::clamp_limit(limit);
        let mut items = Vec::new(&env);
        let mut count = 0;
        let mut last_id = 0;

        for (id, policy) in policies.iter() {
            if id <= cursor {
                continue;
            }
            if policy.active && policy.owner == owner {
                items.push_back(policy);
                count += 1;
                last_id = id;
                if count >= limit {
                    break;
                }
            }
        }

        // Determine if there are more items after the last one returned
        let mut next_cursor = 0;
        if count >= limit {
            for (id, policy) in policies.iter() {
                if id > last_id && policy.active && policy.owner == owner {
                    next_cursor = last_id;
                    break;
                }
            }
        }

        PolicyPage {
            items,
            next_cursor,
            count,
        }
    }

    /// Get total monthly premium for all active policies of an owner
    ///
    /// # Arguments
    /// * `owner` - Address of the policy owner
    ///
    /// # Returns
    /// Total monthly premium amount for the owner's active policies
    pub fn get_total_monthly_premium(env: Env, owner: Address) -> i128 {
        Self::require_initialized(&env).unwrap();
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

        let mut policies: Map<u32, InsurancePolicy> = env.storage().instance().get(&symbol_short!("POLICIES")).unwrap_or_else(|| Map::new(&env));
        let mut policy = policies
            .get(policy_id)
            .ok_or(InsuranceError::PolicyNotFound)?;

        if policy.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }

        let was_active = policy.active;
        policy.active = false;
        let policy_external_ref = policy.external_ref.clone();
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

        Ok(true)
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
    ) -> Result<bool, InsuranceError> {
        caller.require_auth();

        Self::extend_instance_ttl(&env);
        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies.get(policy_id).ok_or(InsuranceError::PolicyNotFound)?;
        if policy.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }

        policy.external_ref = external_ref.clone();
        policies.set(policy_id, policy);
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);

        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::ExternalRefUpdated),
            (policy_id, caller, external_ref),
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

        let mut policies: Map<u32, InsurancePolicy> = env.storage().instance().get(&symbol_short!("POLICIES")).unwrap_or_else(|| Map::new(&env));

        let mut policy = policies
            .get(policy_id)
            .ok_or(InsuranceError::PolicyNotFound)?;

        if policy.owner != owner {
            return Err(InsuranceError::Unauthorized);
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
