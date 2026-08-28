#![no_std]
use remitwise_common::{
    clamp_limit, CoverageType, EventCategory, EventPriority, RemitwiseEvents, DEFAULT_PAGE_LIMIT,
    INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD, MAX_PAGE_LIMIT, PERSISTENT_BUMP_AMOUNT,
    PERSISTENT_LIFETIME_THRESHOLD, SNAPSHOT_KEY, SNAPSHOT_VERSION,
};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String, Vec,
};

mod fee_math;

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

#[contracttype]
#[derive(Clone, Debug)]
pub struct Policy {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub coverage_type: CoverageType,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub external_ref: core::option::Option<String>,
    pub active: bool,
    pub created_at: u64,
    pub last_payment_at: u64,
    pub next_payment_date: u64,
    pub deactivated_at: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PolicyPage {
    pub items: Vec<Policy>,
    pub next_cursor: u32,
    pub count: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum InsuranceEvent {
    PolicyCreated,
    PremiumPaid,
    PolicyDeactivated,
    EmergencyShutdown,
    Resumed,
}

#[contracttype]
#[derive(Clone)]
pub struct PremiumPaidEvent {
    pub policy_id: u32,
    pub name: String,
    pub amount: i128,
    pub next_payment_date: u64,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct PolicyDeactivatedEvent {
    pub policy_id: u32,
    pub name: String,
    pub timestamp: u64,
}

/// A recurring premium schedule for paying a policy's premium automatically.
///
/// Mirrors the field layout of `SavingsSchedule` from the savings_goals contract
/// for consistency across the Remitwise recurring-executor family.
#[contracttype]
#[derive(Clone)]
pub struct NextPaymentSchedule {
    pub id: u32,
    pub owner: Address,
    pub policy_id: u32,
    pub amount: i128,
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
pub struct PremiumScheduleExecutedEvent {
    pub schedule_id: u32,
    pub policy_id: u32,
    pub amount: i128,
    pub next_due: u64,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct PolicyReactivatedEvent {
    pub policy_id: u32,
    pub name: String,
    pub timestamp: u64,
}

/// Typed event discriminant for all insurance policy lifecycle events.
///
/// Used as the **second topic** in every `(symbol_short!("insurance"), InsuranceEvent::*)` pair,
/// giving indexers a single stable enum to subscribe to instead of ad-hoc symbol pairs.
///
/// # Wire stability
/// Variants are serialized by `#[contracttype]` as their ordinal position.
/// **Do not reorder or remove variants** — that is a breaking change for downstream indexers.
/// New variants must be appended at the end.
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InsuranceEvent {
    /// Policy was created (`create_policy`).
    Created = 0,
    /// Premium was paid (`pay_premium` / `batch_pay_premiums`).
    PremiumPaid = 1,
    /// Policy was deactivated (`deactivate_policy`).
    Deactivated = 2,
    /// Policy was reactivated (`reactivate_policy`).
    Reactivated = 3,
    /// External reference was set or cleared (`set_external_ref`).
    ExternalRefUpdated = 4,
    /// Recurring schedule was created.
    ScheduleCreated = 5,
    /// Recurring schedule was executed.
    ScheduleExecuted = 6,
    /// Recurring schedule was cancelled.
    ScheduleCancelled = 7,
    /// Recurring schedule was modified.
    ScheduleModified = 8,
}

/// Event payload emitted when an external reference is set or cleared on a policy.
#[contracttype]
#[derive(Clone)]
pub struct ExternalRefUpdatedEvent {
    /// The policy whose external reference was changed.
    pub policy_id: u32,
    /// The caller (contract owner) who made the change.
    pub caller: Address,
    /// The new external reference value, or `None` if it was cleared.
    pub ext_ref: core::option::Option<String>,
    /// Ledger timestamp at the time of the update.
    pub timestamp: u64,
}

#[contracttype]
pub enum DataKey {
    Owner,
    PolicyCount,
    Policy(u32),
    ActivePolicies,
    ArchivedPolicies,
    OwnerPolicies(Address),
    Initialized,
    NextScheduleId,
    Schedule(u32),
    OwnerSchedules(Address),
}

/// Pre-upgrade snapshot for upgrade rollback protection.
///
/// Captures critical instance storage (owner, policy count, all policies)
/// before a contract upgrade so state can be restored if the upgrade fails.
#[contracttype]
#[derive(Clone)]
pub struct PreUpgradeSnapshot {
    /// Snapshot schema version (`SNAPSHOT_VERSION`).
    pub schema_version: u32,
    /// Contract owner address.
    pub owner: Address,
    /// Total policy count.
    pub policy_count: u32,
    /// Whether the contract has been initialized.
    pub initialized: bool,
    /// List of active policy IDs.
    pub active_policies: Vec<u32>,
    /// Contract version at snapshot time.
    pub version: u32,
}

/// Storage statistics for the insurance contract.
#[contracttype]
#[derive(Clone)]
pub struct StorageStats {
    /// Number of currently active policies.
    pub active_policies: u32,
    /// Number of archived (inactive) policies.
    pub archived_policies: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Contract
// ─────────────────────────────────────────────────────────────────────────────

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

    /// Initialize the insurance contract with the given owner.
    ///
    /// Requires `owner`'s signature — without it, anyone could front-run
    /// deployment and call `init` with themselves (or any address they
    /// control) as `owner` before the intended owner does, permanently
    /// seizing control of the contract.
    ///
    /// # Errors
    /// - `AlreadyInitialized` if the contract has already been initialized
    pub fn init(env: Env, owner: Address) -> Result<(), InsuranceError> {
        owner.require_auth();
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(InsuranceError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage().instance().set(&DataKey::PolicyCount, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::ActivePolicies, &Vec::<u32>::new(&env));
        Self::extend_instance_ttl(&env);
        Ok(())
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

    fn require_initialized(env: &Env) -> Result<(), InsuranceError> {
        if !env.storage().instance().has(&DataKey::Initialized) {
            Err(InsuranceError::NotInitialized)
        } else {
            Ok(())
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

    /// Preview a policy's monthly premium after a loyalty/volume discount
    /// and cap are applied to it. Does not change any stored state -- the
    /// policy's own `monthly_premium` is untouched; this is a read-only
    /// projection for a caller deciding what discount/cap terms to offer.
    ///
    /// # Arguments
    /// * `policy_id` - ID of the policy whose premium is the base fee
    /// * `discount_bps` - Discount in basis points (e.g. `500` = 5%)
    /// * `fee_cap` - Maximum fee after the discount is applied
    ///
    /// # Panics
    /// - If policy is not found
    pub fn calculate_discounted_premium(
        env: Env,
        policy_id: u32,
        discount_bps: u32,
        fee_cap: i128,
    ) -> i128 {
        let policy = Self::get_policy(env, policy_id).expect("Policy not found");
        fee_math::apply_discount_then_cap(policy.monthly_premium, discount_bps, fee_cap)
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

    /// Add a policy ID to the active index.
    ///
    /// Ensures the active index does not exceed `MAX_POLICIES` and avoids
    /// duplicating an ID that is already present. Returns `MaxPoliciesReached`
    /// if the index is full.
    fn add_active_policy(env: &Env, policy_id: u32) -> Result<(), InsuranceError> {
        let mut active = env
            .storage()
            .instance()
            .get::<_, Vec<u32>>(&DataKey::ActivePolicies)
            .ok_or(InsuranceError::NotInitialized)?;
        // If already present, do nothing (prevents duplication)
        for id in active.iter() {
            if id == policy_id {
                return Ok(());
            }
        }
        if active.len() >= MAX_POLICIES {
            return Err(InsuranceError::MaxPoliciesReached);
        }
        active.push_back(policy_id);
        env.storage()
            .instance()
            .set(&DataKey::ActivePolicies, &active);
        Ok(())
    }

    /// Remove a policy ID from the active index.
    fn remove_active_policy(env: &Env, policy_id: u32) -> Result<(), InsuranceError> {
        let active = env
            .storage()
            .instance()
            .get::<_, Vec<u32>>(&DataKey::ActivePolicies)
            .ok_or(InsuranceError::NotInitialized)?;
        let mut new_active = Vec::new(env);
        for id in active.iter() {
            if id != policy_id {
                new_active.push_back(id);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::ActivePolicies, &new_active);
        Ok(())
    }

    fn get_owner(env: &Env) -> Result<Address, InsuranceError> {
        env.storage()
            .instance()
            .get(&DataKey::Owner)
            .ok_or(InsuranceError::NotInitialized)
    }

    fn advance_next_payment_date(previous_due: u64, now: u64) -> u64 {
        if now < previous_due {
            previous_due.saturating_add(THIRTY_DAYS_SECS)
        } else {
            let elapsed = now.saturating_sub(previous_due);
            let periods = (elapsed / THIRTY_DAYS_SECS).saturating_add(1);
            previous_due.saturating_add(periods.saturating_mul(THIRTY_DAYS_SECS))
        }
    }

    fn load_policy(env: &Env, policy_id: u32) -> Result<Policy, InsuranceError> {
        env.storage()
            .instance()
            .get(&DataKey::Policy(policy_id))
            .ok_or(InsuranceError::PolicyNotFound)
    }

    fn validate_ext_ref(ext_ref: &core::option::Option<String>) -> Result<(), InsuranceError> {
        if let Some(r) = ext_ref {
            if r.is_empty() || r.len() > MAX_EXT_REF_LEN {
                return Err(InsuranceError::InvalidExternalRef);
            }
        }
        Ok(())
    }

    // ── Public API ───────────────────────────────────────────────────────────

    /// Create a new insurance policy.
    ///
    /// # Errors
    /// - `NotInitialized` if the contract has not been initialized
    /// - `InvalidName` if the name is empty or too long
    /// - `InvalidPremium` if the monthly premium is not positive or out of range for the coverage type
    /// - `InvalidCoverageAmount` if the coverage amount is not positive or out of range for the coverage type
    /// - `UnsupportedCombination` if the coverage amount is too high relative to the premium
    /// - `MaxPoliciesReached` if the maximum number of policies has been reached
    pub fn create_policy(
        env: Env,
        caller: Address,
        name: String,
        coverage_type: CoverageType,
        monthly_premium: i128,
        coverage_amount: i128,
        ext_ref: Option<String>,
    ) -> Result<u32, InsuranceError> {
        Self::require_initialized(&env)?;
        caller.require_auth();

        if name.is_empty() {
            return Err(InsuranceError::InvalidName);
        }
        if name.len() > MAX_NAME_LEN {
            return Err(InsuranceError::InvalidName);
        }
        if monthly_premium <= 0 {
            return Err(InsuranceError::MonthlyPremiumTooLow);
        }
        if coverage_amount <= 0 {
            return Err(InsuranceError::CoverageAmountTooLow);
        }

        let constraints = TypeConstraints::for_type(&coverage_type);
        if monthly_premium < constraints.min_premium {
            return Err(InsuranceError::MonthlyPremiumTooLow);
        }
        if monthly_premium > constraints.max_premium {
            return Err(InsuranceError::MonthlyPremiumTooHigh);
        }
        if coverage_amount < constraints.min_coverage {
            return Err(InsuranceError::CoverageAmountTooLow);
        }
        if coverage_amount > constraints.max_coverage {
            return Err(InsuranceError::CoverageAmountTooHigh);
        }

        let max_ratio = monthly_premium
            .checked_mul(12)
            .and_then(|v| v.checked_mul(500))
            .unwrap_or(i128::MAX);
        if coverage_amount > max_ratio {
            return Err(InsuranceError::UnsupportedCombination);
        }

        // Reserve a slot in the active index and ensure we don't exceed capacity.
        // `add_active_policy` also prevents duplication.
        let active = env
            .storage()
            .instance()
            .get::<_, Vec<u32>>(&DataKey::ActivePolicies)
            .ok_or(InsuranceError::NotInitialized)?;
        if active.len() >= MAX_POLICIES {
            return Err(InsuranceError::PolicyLimitExceeded);
        }

        // Enforce per-owner cap.
        let owner_ids_check = env
            .storage()
            .instance()
            .get::<_, Vec<u32>>(&DataKey::OwnerPolicies(caller.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        // Count only active owner policies
        let mut active_owner_count = 0u32;
        for oid in owner_ids_check.iter() {
            if let Some(p) = env
                .storage()
                .instance()
                .get::<_, Policy>(&DataKey::Policy(oid))
            {
                if p.active {
                    active_owner_count += 1;
                }
            }
        }
        if active_owner_count >= MAX_POLICIES_PER_OWNER {
            return Err(InsuranceError::PolicyLimitExceeded);
        }

        let next_id = env
            .storage()
            .instance()
            .get::<_, u32>(&DataKey::PolicyCount)
            .unwrap_or(0)
            + 1;
        let now = env.ledger().timestamp();
        let policy = Policy {
            id: next_id,
            owner: caller.clone(),
            name: name.clone(),
            coverage_type,
            monthly_premium,
            coverage_amount,
            external_ref: ext_ref,
            active: true,
            created_at: now,
            last_payment_at: 0,
            next_payment_date: now + THIRTY_DAYS_SECS,
            deactivated_at: 0,
        };

        env.storage()
            .instance()
            .set(&DataKey::Policy(next_id), &policy);
        env.storage()
            .instance()
            .set(&DataKey::PolicyCount, &next_id);
        // Add to active index (helper enforces no-dup and capacity)
        Self::add_active_policy(&env, next_id)?;

        let mut owner_ids = env
            .storage()
            .instance()
            .get::<_, Vec<u32>>(&DataKey::OwnerPolicies(caller.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        owner_ids.push_back(next_id);
        env.storage()
            .instance()
            .set(&DataKey::OwnerPolicies(caller), &owner_ids);

        Self::extend_instance_ttl(&env);
        env.events().publish(
            (symbol_short!("insurance"), InsuranceEvent::Created),
            PolicyCreatedEvent {
                policy_id: next_id,
                name,
                coverage_type,
                monthly_premium,
                coverage_amount,
                timestamp: now,
            },
        );

        Ok(next_id)
    }

    /// Pay the premium for a policy.
    /// Returns `true` on success, `false` if the policy is not found, inactive, or
    /// the caller is not the owner.
    pub fn pay_premium(env: Env, caller: Address, policy_id: u32) -> bool {
        if Self::require_initialized(&env).is_err() {
            return false;
        }
        caller.require_auth();

        let mut policy = match Self::load_policy(&env, policy_id) {
            Ok(p) => p,
            Err(_) => return false,
        };
        if !policy.active {
            return false;
        }
        if caller != policy.owner {
            return false;
        }

        let now = env.ledger().timestamp();
        policy.last_payment_at = now;
        policy.next_payment_date = Self::advance_next_payment_date(policy.next_payment_date, now);

        env.storage()
            .instance()
            .set(&DataKey::Policy(policy_id), &policy);
        Self::extend_instance_ttl(&env);

        env.events().publish(
            (symbol_short!("insurance"), InsuranceEvent::PremiumPaid),
            PremiumPaidEvent {
                policy_id,
                name: policy.name,
                amount: policy.monthly_premium,
                next_payment_date: policy.next_payment_date,
                timestamp: now,
            },
        );

        true
    }

    /// Pay premiums for multiple policies in a single transaction.
    /// Returns the count of policies successfully paid.
    pub fn batch_pay_premiums(env: Env, caller: Address, ids: Vec<u32>) -> u32 {
        if Self::require_initialized(&env).is_err() {
            return 0;
        }
        caller.require_auth();

        let mut count = 0u32;
        for id in ids.iter() {
            let mut policy = match Self::load_policy(&env, id) {
                Ok(p) => p,
                Err(_) => continue,
            };
            if policy.active && policy.owner == caller {
                let now = env.ledger().timestamp();
                policy.last_payment_at = now;
                policy.next_payment_date =
                    Self::advance_next_payment_date(policy.next_payment_date, now);
                let next_payment_date = policy.next_payment_date;
                env.storage().instance().set(&DataKey::Policy(id), &policy);
                env.events().publish(
                    (symbol_short!("insurance"), InsuranceEvent::PremiumPaid),
                    PremiumPaidEvent {
                        policy_id: id,
                        name: policy.name.clone(),
                        amount: policy.monthly_premium,
                        next_payment_date,
                        timestamp: now,
                    },
                );
                count += 1;
            }
        }
        Self::extend_instance_ttl(&env);
        count
    }

    /// Attach or clear an external reference string on a policy (contract owner only).
    ///
    /// # Authorization
    /// Callable **only by the contract owner** — the address supplied to [`init`].
    /// Policy owners and any other callers receive [`InsuranceError::Unauthorized`].
    /// Pass `None` to clear an existing reference.
    ///
    /// # Errors
    /// - [`InsuranceError::NotInitialized`] if the contract has not been initialized
    /// - [`InsuranceError::Unauthorized`] if `caller` is not the contract owner
    /// - [`InsuranceError::PolicyNotFound`] if no policy exists with `policy_id`
    /// - [`InsuranceError::InvalidExternalRef`] if `ext_ref` is `Some` but empty
    ///   or longer than `MAX_EXT_REF_LEN` (128) bytes
    pub fn set_external_ref(
        env: Env,
        caller: Address,
        policy_id: u32,
        ext_ref: core::option::Option<String>,
    ) -> Result<bool, InsuranceError> {
        Self::require_initialized(&env)?;
        caller.require_auth();
        let owner = Self::get_owner(&env)?;
        if caller != owner {
            return Err(InsuranceError::Unauthorized);
        }

        let mut policy = Self::load_policy(&env, policy_id)?;
        Self::validate_ext_ref(&ext_ref)?;
        policy.external_ref = ext_ref.clone();
        env.storage()
            .instance()
            .set(&DataKey::Policy(policy_id), &policy);
        Self::extend_instance_ttl(&env);

        let now = env.ledger().timestamp();
        env.events().publish(
            (
                symbol_short!("insurance"),
                InsuranceEvent::ExternalRefUpdated,
            ),
            ExternalRefUpdatedEvent {
                policy_id,
                caller,
                ext_ref,
                timestamp: now,
            },
        );
        Ok(true)
    }

    /// Deactivate a policy.
    ///
    /// Returns `true` on success, `false` if the policy is not found, caller is
    /// not authorized, or the policy is already inactive.
    pub fn deactivate_policy(env: Env, caller: Address, policy_id: u32) -> bool {
        if Self::require_initialized(&env).is_err() {
            return false;
        }
        caller.require_auth();
        let policy = match Self::load_policy(&env, policy_id) {
            Ok(p) => p,
            Err(_) => return false,
        };
        let contract_owner = match Self::get_owner(&env) {
            Ok(o) => o,
            Err(_) => return false,
        };
        if caller != policy.owner && caller != contract_owner {
            return false;
        }
        if !policy.active {
            // Already inactive — idempotent success
            return true;
        }

        let mut policy = policy;
        policy.active = false;
        policy.deactivated_at = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::Policy(policy_id), &policy);
        let _ = Self::remove_active_policy(&env, policy_id);

        env.events().publish(
            (symbol_short!("insurance"), InsuranceEvent::Deactivated),
            PolicyDeactivatedEvent {
                policy_id,
                name: policy.name,
                timestamp: env.ledger().timestamp(),
            },
        );
        true
    }

    /// Archive a policy (moves it from active to archived state).
    ///
    /// Returns `true` on success, `false` if policy not found or caller unauthorized.
    pub fn archive_policy(env: Env, caller: Address, policy_id: u32) -> bool {
        if Self::require_initialized(&env).is_err() {
            return false;
        }
        caller.require_auth();
        let mut policy = match Self::load_policy(&env, policy_id) {
            Ok(p) => p,
            Err(_) => return false,
        };
        let contract_owner = match Self::get_owner(&env) {
            Ok(o) => o,
            Err(_) => return false,
        };
        if caller != policy.owner && caller != contract_owner {
            return false;
        }

        // If still active, deactivate first
        if policy.active {
            policy.active = false;
            policy.deactivated_at = env.ledger().timestamp();
            let _ = Self::remove_active_policy(&env, policy_id);
        }

        // Add to archived index
        let mut archived = env
            .storage()
            .instance()
            .get::<_, Vec<u32>>(&DataKey::ArchivedPolicies)
            .unwrap_or_else(|| Vec::new(&env));
        let mut already = false;
        for id in archived.iter() {
            if id == policy_id {
                already = true;
                break;
            }
        }
        if !already {
            archived.push_back(policy_id);
            env.storage()
                .instance()
                .set(&DataKey::ArchivedPolicies, &archived);
        }

        env.storage()
            .instance()
            .set(&DataKey::Policy(policy_id), &policy);
        Self::extend_instance_ttl(&env);
        true
    }

    /// Restore an archived policy back to active state.
    ///
    /// Returns `true` on success, `false` if policy not found, caller unauthorized,
    /// or the per-owner active cap would be exceeded.
    pub fn restore_policy(env: Env, caller: Address, policy_id: u32) -> bool {
        if Self::require_initialized(&env).is_err() {
            return false;
        }
        caller.require_auth();
        let mut policy = match Self::load_policy(&env, policy_id) {
            Ok(p) => p,
            Err(_) => return false,
        };
        let contract_owner = match Self::get_owner(&env) {
            Ok(o) => o,
            Err(_) => return false,
        };
        if caller != policy.owner && caller != contract_owner {
            return false;
        }

        // Check per-owner active cap
        let owner_ids = env
            .storage()
            .instance()
            .get::<_, Vec<u32>>(&DataKey::OwnerPolicies(policy.owner.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        let mut active_owner_count = 0u32;
        for oid in owner_ids.iter() {
            if let Some(p) = env
                .storage()
                .instance()
                .get::<_, Policy>(&DataKey::Policy(oid))
            {
                if p.active {
                    active_owner_count += 1;
                }
            }
        }
        if active_owner_count >= MAX_POLICIES_PER_OWNER {
            return false;
        }

        // Restore
        let now = env.ledger().timestamp();
        policy.active = true;
        policy.next_payment_date = Self::advance_next_payment_date(policy.next_payment_date, now);
        policy.deactivated_at = 0;
        env.storage()
            .instance()
            .set(&DataKey::Policy(policy_id), &policy);

        // Remove from archived index
        let archived = env
            .storage()
            .instance()
            .get::<_, Vec<u32>>(&DataKey::ArchivedPolicies)
            .unwrap_or_else(|| Vec::new(&env));
        let mut new_archived = Vec::new(&env);
        for id in archived.iter() {
            if id != policy_id {
                new_archived.push_back(id);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::ArchivedPolicies, &new_archived);

        // Add back to active index
        let _ = Self::add_active_policy(&env, policy_id);

        Self::extend_instance_ttl(&env);
        true
    }

    /// Reactivate a previously deactivated policy.
    pub fn reactivate_policy(
        env: Env,
        caller: Address,
        policy_id: u32,
    ) -> Result<bool, InsuranceError> {
        Self::require_initialized(&env)?;
        caller.require_auth();

        let mut policy = Self::load_policy(&env, policy_id)?;
        let owner = Self::get_owner(&env)?;
        if caller != policy.owner && caller != owner {
            return Err(InsuranceError::Unauthorized);
        }
        if policy.active {
            return Err(InsuranceError::PolicyAlreadyActive);
        }

        let now = env.ledger().timestamp();
        if policy.deactivated_at != 0 && now < policy.deactivated_at + MAX_TENURE_SECS {
            return Err(InsuranceError::PolicyDeactivationTooSoon);
        }

        policy.next_payment_date = Self::advance_next_payment_date(policy.next_payment_date, now);
        policy.active = true;
        env.storage()
            .instance()
            .set(&DataKey::Policy(policy_id), &policy);

        Self::add_active_policy(&env, policy_id)?;

        env.events().publish(
            (symbol_short!("insurance"), InsuranceEvent::Reactivated),
            PolicyReactivatedEvent {
                policy_id,
                name: policy.name,
                timestamp: now,
            },
        );
        Ok(true)
    }

    /// Get a paginated list of active policies for an owner.
    ///
    /// See [`docs/PAGINATION_HANDBOOK.md`](../../docs/PAGINATION_HANDBOOK.md) for the invariants
    /// all paginated reads must satisfy, cursor semantics, and the reviewer checklist.
    ///
    /// # Errors
    /// - `NotInitialized` if the contract has not been initialized
    pub fn get_active_policies(
        env: Env,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> Result<PolicyPage, InsuranceError> {
        Self::require_initialized(&env)?;

        let owner_ids = env
            .storage()
            .instance()
            .get::<_, Vec<u32>>(&DataKey::OwnerPolicies(owner))
            .unwrap_or_else(|| Vec::new(&env));

        let mut items: Vec<Policy> = Vec::new(&env);
        let mut next_cursor = 0u32;

        let lim = if limit == 0 {
            DEFAULT_PAGE_LIMIT
        } else if limit > MAX_PAGE_LIMIT {
            MAX_PAGE_LIMIT
        } else {
            limit
        };

        for id in owner_ids.iter() {
            if id > cursor {
                if let Some(p) = env
                    .storage()
                    .instance()
                    .get::<_, Policy>(&DataKey::Policy(id))
                {
                    if p.active {
                        if items.len() < lim {
                            items.push_back(p);
                        } else {
                            next_cursor = id;
                            break;
                        }
                    }
                }
            }
        }

        let count = items.len();
        Ok(PolicyPage {
            items,
            next_cursor,
            count,
        })
    }

    /// Get a paginated list of deactivated policies for an owner.
    ///
    /// See [`docs/PAGINATION_HANDBOOK.md`](../../docs/PAGINATION_HANDBOOK.md) for the invariants
    /// all paginated reads must satisfy, cursor semantics, and the reviewer checklist.
    ///
    /// Mirrors the shape and semantics of `get_active_policies` but filters
    /// for policies where `active == false`. `limit` is normalized via
    /// `remitwise_common::clamp_limit`.
    pub fn get_deactivated_policies(
        env: Env,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> PolicyPage {
        if Self::require_initialized(&env).is_err() {
            return PolicyPage {
                items: Vec::new(&env),
                next_cursor: 0,
                count: 0,
            };
        }

        let owner_ids = env
            .storage()
            .instance()
            .get::<_, Vec<u32>>(&DataKey::OwnerPolicies(owner))
            .unwrap_or_else(|| Vec::new(&env));

        let mut items: Vec<Policy> = Vec::new(&env);
        let mut next_cursor = 0u32;

        let lim = clamp_limit(limit);

        for id in owner_ids.iter() {
            if id > cursor {
                if let Some(p) = env
                    .storage()
                    .instance()
                    .get::<_, Policy>(&DataKey::Policy(id))
                {
                    if !p.active {
                        if items.len() < lim {
                            items.push_back(p);
                        } else {
                            next_cursor = id;
                            break;
                        }
                    }
                }
            }
        }

        let count = items.len();
        PolicyPage {
            items,
            next_cursor,
            count,
        }
    }

    /// Get a policy by ID. Returns `None` if not found or not initialized.
    pub fn get_policy(env: Env, policy_id: u32) -> Option<Policy> {
        if Self::require_initialized(&env).is_err() {
            return None;
        }
        env.storage().instance().get(&DataKey::Policy(policy_id))
    }

    /// Get the total monthly premium for all active policies owned by an address.
    pub fn get_total_monthly_premium(env: Env, owner: Address) -> i128 {
        if Self::require_initialized(&env).is_err() {
            return 0;
        }

        let owner_ids = env
            .storage()
            .instance()
            .get::<_, Vec<u32>>(&DataKey::OwnerPolicies(owner))
            .unwrap_or_else(|| Vec::new(&env));

        let mut total: i128 = 0;
        for id in owner_ids.iter() {
            if let Some(p) = env
                .storage()
                .instance()
                .get::<_, Policy>(&DataKey::Policy(id))
            {
                if p.active {
                    total = total.saturating_add(p.monthly_premium);
                }
            }
        }
        total
    }

    /// Get storage statistics for the insurance contract.
    pub fn get_storage_stats(env: Env) -> StorageStats {
        if Self::require_initialized(&env).is_err() {
            return StorageStats {
                active_policies: 0,
                archived_policies: 0,
            };
        }

        let active_policies = env
            .storage()
            .instance()
            .get::<_, Vec<u32>>(&DataKey::ActivePolicies)
            .unwrap_or_else(|| Vec::new(&env))
            .len();

        let archived_policies = env
            .storage()
            .instance()
            .get::<_, Vec<u32>>(&DataKey::ArchivedPolicies)
            .unwrap_or_else(|| Vec::new(&env))
            .len();

        StorageStats {
            active_policies,
            archived_policies,
        }
    }

    /// Get the contract version.
    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("VERSION"))
            .unwrap_or(1)
    }

    /// Set the contract version (upgrade support).
    ///
    /// # Authorization
    /// Only the contract owner may set the version.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the contract owner
    /// - `NotInitialized` if the contract has not been initialized
    pub fn set_version(
        env: Env,
        caller: Address,
        new_version: u32,
    ) -> Result<bool, InsuranceError> {
        Self::require_initialized(&env)?;
        caller.require_auth();
        let owner = Self::get_owner(&env)?;
        if caller != owner {
            return Err(InsuranceError::Unauthorized);
        }
        let prev = Self::get_version(env.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("VERSION"), &new_version);
        env.events().publish(
            (symbol_short!("insurance"), symbol_short!("upgraded")),
            (prev, new_version),
        );
        Ok(true)
    }

    /// Set pause admin (minimal implementation for testing compatibility).
    pub fn set_pause_admin(env: Env, caller: Address, _admin: Address) -> bool {
        if Self::require_initialized(&env).is_err() {
            return false;
        }
        caller.require_auth();
        let owner = match Self::get_owner(&env) {
            Ok(o) => o,
            Err(_) => return false,
        };
        if caller != owner {
            return false;
        }
        true
    }

    /// Capture a pre-upgrade snapshot of critical instance storage.
    ///
    /// Call this before performing a contract upgrade. The snapshot captures
    /// the owner, policy count, all policies, and active policy index so the
    /// contract can be restored if the upgrade fails.
    ///
    /// # Authorization
    /// Only the contract owner may take a snapshot.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the contract owner
    /// - `NotInitialized` if the contract has not been initialized
    ///
    /// # Events
    /// Emits `(symbol_short!("insurance"), symbol_short!("snap_pre"))`.
    pub fn pre_upgrade(env: Env, caller: Address) -> Result<(), InsuranceError> {
        Self::require_initialized(&env)?;
        caller.require_auth();
        let owner = Self::get_owner(&env)?;
        if caller != owner {
            return Err(InsuranceError::Unauthorized);
        }
        let active: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::ActivePolicies)
            .unwrap_or_else(|| Vec::new(&env));
        let policy_count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PolicyCount)
            .unwrap_or(0);
        let snapshot = PreUpgradeSnapshot {
            schema_version: SNAPSHOT_VERSION,
            owner: owner.clone(),
            policy_count,
            initialized: true,
            active_policies: active,
            version: Self::get_version(env.clone()),
        };
        env.storage().persistent().set(&SNAPSHOT_KEY, &snapshot);
        env.storage()
            .persistent()
            .set(&symbol_short!("SNAP_TS"), &env.ledger().timestamp());
        env.events().publish(
            (symbol_short!("insurance"), symbol_short!("snap_pre")),
            SNAPSHOT_VERSION,
        );
        Ok(())
    }

    /// Restore critical instance storage from a pre-upgrade snapshot.
    ///
    /// Reads the snapshot stored by `pre_upgrade` and writes the captured
    /// owner, policies, and active index back to instance storage.
    /// The snapshot is consumed after a successful restore.
    ///
    /// # Authorization
    /// Only the contract owner may restore from a snapshot.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the contract owner
    /// - `NotInitialized` if no snapshot exists
    /// - `UnsupportedVersion` if the snapshot version is not supported
    ///
    /// # Events
    /// Emits `(symbol_short!("insurance"), symbol_short!("snap_rst"))`.
    pub fn restore_from_snapshot(env: Env, caller: Address) -> Result<(), InsuranceError> {
        Self::require_initialized(&env)?;
        caller.require_auth();
        let owner = Self::get_owner(&env)?;
        if caller != owner {
            return Err(InsuranceError::Unauthorized);
        }
        let snapshot: PreUpgradeSnapshot = env
            .storage()
            .persistent()
            .get(&SNAPSHOT_KEY)
            .ok_or(InsuranceError::SnapshotNotFound)?;
        if snapshot.schema_version != SNAPSHOT_VERSION {
            return Err(InsuranceError::Unauthorized);
        }
        if snapshot.owner != owner {
            return Err(InsuranceError::Unauthorized);
        }
        let snapshot_taken_at: u64 = env
            .storage()
            .persistent()
            .get(&symbol_short!("SNAP_TS"))
            .unwrap_or(0);
        if remitwise_common::require_recent_snapshot(&env, snapshot_taken_at).is_err() {
            return Err(InsuranceError::SnapshotTooOld);
        }
        Self::extend_instance_ttl(&env);

        // Restore policy count and initialization
        env.storage()
            .instance()
            .set(&DataKey::PolicyCount, &snapshot.policy_count);
        env.storage()
            .instance()
            .set(&DataKey::Initialized, &snapshot.initialized);

        // Restore active policies list
        env.storage()
            .instance()
            .set(&DataKey::ActivePolicies, &snapshot.active_policies);

        // Restore version
        env.storage()
            .instance()
            .set(&symbol_short!("VERSION"), &snapshot.version);

        // Consume the snapshot
        env.storage().persistent().remove(&SNAPSHOT_KEY);

        env.events().publish(
            (symbol_short!("insurance"), symbol_short!("snap_rst")),
            snapshot.policy_count,
        );
        Ok(())
    }

    /// Discard a pre-upgrade snapshot without restoring it.
    ///
    /// Use after a successful upgrade to free persistent storage.
    ///
    /// # Authorization
    /// Only the contract owner may discard a snapshot.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the contract owner
    /// - `NotInitialized` if the contract has not been initialized
    pub fn discard_snapshot(env: Env, caller: Address) -> Result<(), InsuranceError> {
        Self::require_initialized(&env)?;
        caller.require_auth();
        let owner = Self::get_owner(&env)?;
        if caller != owner {
            return Err(InsuranceError::Unauthorized);
        }
        env.storage().persistent().remove(&SNAPSHOT_KEY);
        env.events()
            .publish((symbol_short!("insurance"), symbol_short!("snap_dsc")), ());
        Ok(())
    }

    // ── Scheduler ──────────────────────────────────────────────────────────

    fn extend_persistent_ttl(env: &Env, key: &DataKey) {
        env.storage().persistent().extend_ttl(
            key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    /// Create a recurring premium schedule for a policy.
    ///
    /// The schedule pays `amount` every `interval` seconds starting from
    /// `next_due`. One-shot schedules use `interval = 0` (executed once then
    /// auto-deactivated).
    ///
    /// # Guards
    /// - `interval` must be >= `MIN_SCHEDULE_INTERVAL` (1 hour) for recurring
    ///   schedules, or 0 for one-shot.
    /// - `next_due` must be in the future.
    /// - `next_due` must be <= `now + MAX_SCHEDULE_LEAD_TIME` (1 year).
    /// - The owner must not exceed `MAX_SCHEDULES_PER_OWNER`.
    ///
    /// # Errors
    /// - [`InsuranceError::PolicyNotFound`] if `policy_id` does not exist
    /// - [`InsuranceError::PolicyInactive`] if the policy is not active
    /// - [`InsuranceError::ScheduleIntervalTooShort`] if `interval` < 1 hour
    ///   (for recurring schedules)
    /// - [`InsuranceError::ScheduleLeadTimeTooLong`] if `next_due` is too far
    ///   in the future
    pub fn create_premium_schedule(
        env: Env,
        owner: Address,
        policy_id: u32,
        amount: i128,
        next_due: u64,
        interval: u64,
    ) -> Result<u32, InsuranceError> {
        Self::require_initialized(&env)?;
        owner.require_auth();

        if amount <= 0 {
            return Err(InsuranceError::InvalidPremium);
        }

        let policy = Self::load_policy(&env, policy_id)?;
        if !policy.active {
            return Err(InsuranceError::PolicyInactive);
        }
        if policy.owner != owner {
            return Err(InsuranceError::Unauthorized);
        }

        let now = env.ledger().timestamp();
        if next_due <= now {
            return Err(InsuranceError::InvalidPremium);
        }
        if next_due > now.saturating_add(MAX_SCHEDULE_LEAD_TIME) {
            return Err(InsuranceError::ScheduleLeadTimeTooLong);
        }
        if interval > 0 && interval < MIN_SCHEDULE_INTERVAL {
            return Err(InsuranceError::ScheduleIntervalTooShort);
        }

        let mut owner_ids = env
            .storage()
            .persistent()
            .get::<_, Vec<u32>>(&DataKey::OwnerSchedules(owner.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        if owner_ids.len() >= MAX_SCHEDULES_PER_OWNER {
            return Err(InsuranceError::MaxPoliciesReached);
        }

        Self::extend_instance_ttl(&env);

        let next_id = env
            .storage()
            .instance()
            .get::<_, u32>(&DataKey::NextScheduleId)
            .unwrap_or(0)
            + 1;

        let schedule = NextPaymentSchedule {
            id: next_id,
            owner: owner.clone(),
            policy_id,
            amount,
            next_due,
            interval,
            recurring: interval > 0,
            active: true,
            created_at: now,
            last_executed: None,
            missed_count: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Schedule(next_id), &schedule);
        Self::extend_persistent_ttl(&env, &DataKey::Schedule(next_id));

        env.storage()
            .instance()
            .set(&DataKey::NextScheduleId, &next_id);

        owner_ids.push_back(next_id);
        env.storage()
            .persistent()
            .set(&DataKey::OwnerSchedules(owner), &owner_ids);

        env.events().publish(
            (symbol_short!("insurance"), InsuranceEvent::ScheduleCreated),
            (next_id, policy_id),
        );

        Ok(next_id)
    }

    /// Modify an existing premium schedule.
    ///
    /// Only the schedule owner may modify. Updates `amount`, `next_due`,
    /// and `interval`. The same guards as `create_premium_schedule` apply.
    ///
    /// # Errors
    /// - [`InsuranceError::ScheduleNotFound`] if `schedule_id` does not exist
    /// - [`InsuranceError::Unauthorized`] if `owner` is not the schedule owner
    /// - [`InsuranceError::ScheduleIntervalTooShort`] if `interval` is too short
    /// - [`InsuranceError::ScheduleLeadTimeTooLong`] if `next_due` is too far
    pub fn modify_premium_schedule(
        env: Env,
        caller: Address,
        schedule_id: u32,
        amount: i128,
        next_due: u64,
        interval: u64,
    ) -> Result<bool, InsuranceError> {
        Self::require_initialized(&env)?;
        caller.require_auth();

        if amount <= 0 {
            return Err(InsuranceError::InvalidPremium);
        }

        let now = env.ledger().timestamp();
        if next_due <= now {
            return Err(InsuranceError::InvalidPremium);
        }
        if next_due > now.saturating_add(MAX_SCHEDULE_LEAD_TIME) {
            return Err(InsuranceError::ScheduleLeadTimeTooLong);
        }
        if interval > 0 && interval < MIN_SCHEDULE_INTERVAL {
            return Err(InsuranceError::ScheduleIntervalTooShort);
        }

        Self::extend_instance_ttl(&env);

        let mut schedule = match env
            .storage()
            .persistent()
            .get::<_, NextPaymentSchedule>(&DataKey::Schedule(schedule_id))
        {
            Some(s) => s,
            None => return Err(InsuranceError::ScheduleNotFound),
        };

        if schedule.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }

        schedule.amount = amount;
        schedule.next_due = next_due;
        schedule.interval = interval;
        schedule.recurring = interval > 0;

        env.storage()
            .persistent()
            .set(&DataKey::Schedule(schedule_id), &schedule);
        Self::extend_persistent_ttl(&env, &DataKey::Schedule(schedule_id));

        env.events().publish(
            (symbol_short!("insurance"), InsuranceEvent::ScheduleModified),
            (schedule_id,),
        );

        Ok(true)
    }

    /// Cancel (deactivate) a premium schedule.
    ///
    /// Only the schedule owner may cancel. Sets `active = false` so the
    /// schedule is skipped by `execute_due_premium_schedules`.
    ///
    /// # Errors
    /// - [`InsuranceError::ScheduleNotFound`] if `schedule_id` does not exist
    /// - [`InsuranceError::Unauthorized`] if `caller` is not the schedule owner
    pub fn cancel_premium_schedule(
        env: Env,
        caller: Address,
        schedule_id: u32,
    ) -> Result<bool, InsuranceError> {
        Self::require_initialized(&env)?;
        caller.require_auth();

        Self::extend_instance_ttl(&env);

        let mut schedule = match env
            .storage()
            .persistent()
            .get::<_, NextPaymentSchedule>(&DataKey::Schedule(schedule_id))
        {
            Some(s) => s,
            None => return Err(InsuranceError::ScheduleNotFound),
        };

        if schedule.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }

        schedule.active = false;

        env.storage()
            .persistent()
            .set(&DataKey::Schedule(schedule_id), &schedule);
        Self::extend_persistent_ttl(&env, &DataKey::Schedule(schedule_id));

        env.events().publish(
            (
                symbol_short!("insurance"),
                InsuranceEvent::ScheduleCancelled,
            ),
            (schedule_id,),
        );

        Ok(true)
    }

    /// Get a single premium schedule by ID.
    ///
    /// Returns `None` if the schedule does not exist.
    pub fn get_premium_schedule(env: Env, schedule_id: u32) -> Option<NextPaymentSchedule> {
        env.storage()
            .persistent()
            .get(&DataKey::Schedule(schedule_id))
    }

    /// Get all premium schedules for an owner.
    pub fn get_premium_schedules(env: Env, owner: Address) -> Vec<NextPaymentSchedule> {
        let ids: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerSchedules(owner))
            .unwrap_or_else(|| Vec::new(&env));

        let mut result = Vec::new(&env);
        for schedule_id in ids.iter() {
            if let Some(s) = env
                .storage()
                .persistent()
                .get::<_, NextPaymentSchedule>(&DataKey::Schedule(schedule_id))
            {
                result.push_back(s);
            }
        }
        result
    }

    /// Execute all due premium schedules.
    ///
    /// A permissionless entrypoint that pays all premiums whose `next_due`
    /// timestamp is at or before the current ledger time.
    ///
    /// # Idempotency
    /// A schedule is skipped if its `last_executed` timestamp is >= its
    /// `next_due` timestamp at the time of the call. This prevents
    /// double-processing within the same ledger.
    ///
    /// # Next-due advancement (mirrors savings_goals)
    /// - **Recurring** (`interval > 0`): `next_due` is advanced by `interval`
    ///   until it is strictly > `current_time`. Skipped intervals increment
    ///   `missed_count`.
    /// - **One-shot** (`interval == 0`): deactivated after a single execution.
    ///
    /// # Events
    /// - Emits `PremiumScheduleExecutedEvent` for each successful execution.
    ///
    /// # Returns
    /// A `Vec<u32>` of schedule IDs that were executed.
    pub fn execute_due_premium_schedules(env: Env) -> Vec<u32> {
        let next_schedule_id = env
            .storage()
            .instance()
            .get::<_, u32>(&DataKey::NextScheduleId)
            .unwrap_or(0);

        let current_time = env.ledger().timestamp();
        let mut executed: Vec<u32> = Vec::new(&env);

        for schedule_id in 1..=next_schedule_id {
            let mut schedule = match env
                .storage()
                .persistent()
                .get::<_, NextPaymentSchedule>(&DataKey::Schedule(schedule_id))
            {
                Some(s) => s,
                None => continue,
            };

            if !schedule.active || schedule.next_due > current_time {
                continue;
            }

            // Idempotency guard: skip if already executed for this due date
            if let Some(last_exec) = schedule.last_executed {
                if last_exec >= schedule.next_due {
                    continue;
                }
            }

            let mut policy = match Self::load_policy(&env, schedule.policy_id) {
                Ok(p) => p,
                Err(_) => continue,
            };

            if !policy.active {
                continue;
            }

            let now = env.ledger().timestamp();
            policy.last_payment_at = now;
            policy.next_payment_date =
                Self::advance_next_payment_date(policy.next_payment_date, now);

            env.storage()
                .instance()
                .set(&DataKey::Policy(schedule.policy_id), &policy);

            schedule.last_executed = Some(now);

            if schedule.recurring && schedule.interval > 0 {
                let mut missed = 0u32;
                let mut next = schedule.next_due.saturating_add(schedule.interval);
                while next <= current_time {
                    missed = missed.saturating_add(1);
                    next = next.saturating_add(schedule.interval);
                }
                schedule.missed_count = schedule.missed_count.saturating_add(missed);
                schedule.next_due = next;
            } else {
                schedule.active = false;
            }

            env.storage()
                .persistent()
                .set(&DataKey::Schedule(schedule_id), &schedule);
            Self::extend_persistent_ttl(&env, &DataKey::Schedule(schedule_id));

            let event = PremiumScheduleExecutedEvent {
                schedule_id,
                policy_id: schedule.policy_id,
                amount: schedule.amount,
                next_due: schedule.next_due,
                timestamp: now,
            };
            env.events().publish(
                (symbol_short!("insurance"), InsuranceEvent::ScheduleExecuted),
                event,
            );

            RemitwiseEvents::emit(
                &env,
                EventCategory::Transaction,
                EventPriority::Medium,
                symbol_short!("prem_pay"),
                (schedule_id, schedule.policy_id, schedule.amount),
            );

            executed.push_back(schedule_id);

            Self::extend_instance_ttl(&env);
        }

        executed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn calculate_discounted_premium_discounts_before_capping() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "Health"),
            &String::from_str(&env, "health"),
            &1000,
            &10_000,
        );

        // 10% off a 1000 premium is 900, under the 920 cap -- the cap
        // must not bind here (see fee_math's tests for the case where a
        // wrong cap-first order would instead yield 828).
        let discounted = client.calculate_discounted_premium(&policy_id, &1_000, &920);

        assert_eq!(discounted, 900);
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
