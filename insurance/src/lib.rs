#![no_std]
use remitwise_common::{
    CoverageType, EventCategory, EventPriority, RemitwiseEvents, DEFAULT_PAGE_LIMIT,
    INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD, MAX_PAGE_LIMIT, PERSISTENT_BUMP_AMOUNT,
    PERSISTENT_LIFETIME_THRESHOLD, SNAPSHOT_KEY, SNAPSHOT_VERSION,
};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol, Vec,
};

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

const THIRTY_DAYS_SECS: u64 = 30 * 24 * 60 * 60;
const MAX_NAME_LEN: u32 = 64;
const MAX_EXT_REF_LEN: u32 = 128;
const MAX_POLICIES: u32 = 1_000;

/// Minimum allowed recurrence interval for repeating premium schedules (1 hour).
const MIN_SCHEDULE_INTERVAL: u64 = 3_600;
/// Maximum allowed lead time for schedule due dates (1 year).
const MAX_SCHEDULE_LEAD_TIME: u64 = 365 * 24 * 3_600;
/// Maximum premium schedules allowed per owner.
const MAX_SCHEDULES_PER_OWNER: u32 = 50;

// ─────────────────────────────────────────────────────────────────────────────
// Error Codes
// ─────────────────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum InsuranceError {
    Unauthorized = 1,
    AlreadyInitialized = 2,
    NotInitialized = 3,
    PolicyNotFound = 4,
    PolicyInactive = 5,
    InvalidName = 6,
    InvalidPremium = 7,
    InvalidCoverageAmount = 8,
    UnsupportedCombination = 9,
    InvalidExternalRef = 10,
    MaxPoliciesReached = 11,
    /// Returned by `reactivate_policy` when the target policy is already active.
    PolicyAlreadyActive = 12,
    /// Returned by `deactivate_policy` when the target policy is already inactive.
    /// Distinct from `PolicyInactive` (which signals a caller trying to act *on*
    /// an inactive policy) — `PolicyAlreadyInactive` signals that the *deactivation
    /// itself* is a no-op because the policy was never active (or was already
    /// deactivated by a prior call).
    PolicyAlreadyInactive = 17,
    /// The requested schedule was not found.
    ScheduleNotFound = 13,
    /// The schedule is inactive (cancelled or deactivated).
    InactiveSchedule = 14,
    /// The schedule interval is below the minimum allowed value (1 hour).
    ScheduleIntervalTooShort = 15,
    /// The schedule lead time exceeds the maximum allowed value (1 year).
    ScheduleLeadTimeTooLong = 16,
}

// ─────────────────────────────────────────────────────────────────────────────
// Data Types
// ─────────────────────────────────────────────────────────────────────────────

/// Per-type premium and coverage constraints (all values in stroops).
struct TypeConstraints {
    min_premium: i128,
    max_premium: i128,
    min_coverage: i128,
    max_coverage: i128,
}

impl TypeConstraints {
    /// Return the allowed premium and coverage bounds for a given [`CoverageType`].
    ///
    /// All values are in **stroops** (1 XLM = 10 000 000 stroops).
    /// `create_policy` uses these bounds to gate [`InsuranceError::InvalidPremium`],
    /// [`InsuranceError::InvalidCoverageAmount`], and [`InsuranceError::UnsupportedCombination`].
    ///
    /// # Per-type bounds table
    ///
    /// | CoverageType | min_premium | max_premium        | min_coverage | max_coverage             |
    /// |--------------|------------:|--------------------|-------------:|--------------------------|
    /// | Health       |           1 | 500 000 000 000    |            1 | 100 000 000 000 000      |
    /// | Life         |           1 | 1 000 000 000 000  |            1 | 500 000 000 000 000      |
    /// | Property     |           1 | 2 000 000 000 000  |            1 | 1 000 000 000 000 000    |
    /// | Auto         |           1 | 750 000 000 000    |            1 | 200 000 000 000 000      |
    /// | Liability    |           1 | 400 000 000 000    |            1 | 50 000 000 000 000       |
    ///
    /// # Overflow safety
    ///
    /// The UnsupportedCombination check (`coverage_amount > premium * 12 * 500`) uses
    /// `checked_mul` and saturates to `i128::MAX` on overflow, so passing values near
    /// `i128::MAX` as the premium does not cause a panic — it simply results in a comparison
    /// against `i128::MAX` which the coverage amount cannot exceed.
    ///
    /// Even the largest `max_premium` (Property: 2 × 10¹²) × 12 × 500 = 1.2 × 10¹⁶,
    /// well within `i128::MAX` (≈ 1.7 × 10³⁸).
    fn for_type(t: &CoverageType) -> Self {
        match t {
            CoverageType::Health => Self {
                min_premium: 1,
                max_premium: 500_000_000_000,
                min_coverage: 1,
                max_coverage: 100_000_000_000_000,
            },
            CoverageType::Life => Self {
                min_premium: 1,
                max_premium: 1_000_000_000_000,
                min_coverage: 1,
                max_coverage: 500_000_000_000_000,
            },
            CoverageType::Property => Self {
                min_premium: 1,
                max_premium: 2_000_000_000_000,
                min_coverage: 1,
                max_coverage: 1_000_000_000_000_000,
            },
            CoverageType::Auto => Self {
                min_premium: 1,
                max_premium: 750_000_000_000,
                min_coverage: 1,
                max_coverage: 200_000_000_000_000,
            },
            CoverageType::Liability => Self {
                min_premium: 1,
                max_premium: 400_000_000_000,
                min_coverage: 1,
                max_coverage: 50_000_000_000_000,
            },
        }
    }
}

#[contracttype]
#[derive(Clone)]
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
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PolicyPage {
    pub items: Vec<u32>,
    pub next_cursor: u32,
    pub count: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct PolicyCreatedEvent {
    pub policy_id: u32,
    pub name: String,
    pub coverage_type: CoverageType,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub timestamp: u64,
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

#[contracttype]
pub enum DataKey {
    Owner,
    PolicyCount,
    Policy(u32),
    ActivePolicies,
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

// ─────────────────────────────────────────────────────────────────────────────
// Contract
// ─────────────────────────────────────────────────────────────────────────────

#[contract]
pub struct Insurance;

#[contractimpl]
impl Insurance {
    // ── Initialization ───────────────────────────────────────────────────────

    /// Initialize the insurance contract with the given owner.
    ///
    /// # Errors
    /// - `AlreadyInitialized` if the contract has already been initialized
    pub fn init(env: Env, owner: Address) -> Result<(), InsuranceError> {
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

    // ── Internal helpers ─────────────────────────────────────────────────────

    fn require_initialized(env: &Env) -> Result<(), InsuranceError> {
        if !env.storage().instance().has(&DataKey::Initialized) {
            Err(InsuranceError::NotInitialized)
        } else {
            Ok(())
        }
    }

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
        let mut new_active = Vec::new(&env);
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
            return Err(InsuranceError::InvalidPremium);
        }
        if coverage_amount <= 0 {
            return Err(InsuranceError::InvalidCoverageAmount);
        }

        let constraints = TypeConstraints::for_type(&coverage_type);
        if monthly_premium < constraints.min_premium || monthly_premium > constraints.max_premium {
            return Err(InsuranceError::InvalidPremium);
        }
        if coverage_amount < constraints.min_coverage || coverage_amount > constraints.max_coverage
        {
            return Err(InsuranceError::InvalidCoverageAmount);
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
            return Err(InsuranceError::MaxPoliciesReached);
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
            external_ref: core::option::Option::None,
            active: true,
            created_at: now,
            last_payment_at: 0,
            next_payment_date: now + THIRTY_DAYS_SECS,
        };

        env.storage().instance().set(&DataKey::Policy(next_id), &policy);
        env.storage().instance().set(&DataKey::PolicyCount, &next_id);
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
            (symbol_short!("created"), symbol_short!("policy")),
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
    ///
    /// # Errors
    /// - `NotInitialized` if the contract has not been initialized
    /// - `PolicyNotFound` if the policy does not exist
    /// - `PolicyInactive` if the policy is not active
    /// - `Unauthorized` if the caller is not the policy owner
    pub fn pay_premium(env: Env, caller: Address, policy_id: u32) -> Result<bool, InsuranceError> {
        Self::require_initialized(&env)?;
        caller.require_auth();

        let mut policy = Self::load_policy(&env, policy_id)?;
        if !policy.active {
            return Err(InsuranceError::PolicyInactive);
        }
        if caller != policy.owner {
            return Err(InsuranceError::Unauthorized);
        }

        let now = env.ledger().timestamp();
        policy.last_payment_at = now;
        policy.next_payment_date = Self::advance_next_payment_date(policy.next_payment_date, now);

        env.storage()
            .instance()
            .set(&DataKey::Policy(policy_id), &policy);
        Self::extend_instance_ttl(&env);

        env.events().publish(
            (symbol_short!("paid"), symbol_short!("premium")),
            PremiumPaidEvent {
                policy_id,
                name: policy.name,
                amount: policy.monthly_premium,
                next_payment_date: policy.next_payment_date,
                timestamp: now,
            },
        );

        Ok(true)
    }

    /// Pay premiums for multiple policies in a single transaction.
    ///
    /// # Errors
    /// - `NotInitialized` if the contract has not been initialized
    /// - `PolicyNotFound` if any policy does not exist
    pub fn batch_pay_premiums(
        env: Env,
        caller: Address,
        ids: Vec<u32>,
    ) -> Result<u32, InsuranceError> {
        Self::require_initialized(&env)?;
        caller.require_auth();

        let mut count = 0u32;
        for id in ids.iter() {
            let mut policy = Self::load_policy(&env, id)?;
            if policy.active && policy.owner == caller {
                let now = env.ledger().timestamp();
                policy.last_payment_at = now;
                policy.next_payment_date =
                    Self::advance_next_payment_date(policy.next_payment_date, now);
                let next_payment_date = policy.next_payment_date;
                env.storage().instance().set(&DataKey::Policy(id), &policy);
                env.events().publish(
                    (symbol_short!("paid"), symbol_short!("premium")),
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
        Ok(count)
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
        policy.external_ref = ext_ref;
        env.storage()
            .instance()
            .set(&DataKey::Policy(policy_id), &policy);
        Ok(true)
    }

    /// Deactivate a policy.
    ///
    /// # Authorization
    /// Callable by the **policy owner** (the address that created the policy) or
    /// the **contract owner** (the address supplied to [`init`]). Any other caller
    /// receives [`InsuranceError::Unauthorized`].
    ///
    /// # Errors
    /// - [`InsuranceError::NotInitialized`] if the contract has not been initialized
    /// - [`InsuranceError::PolicyNotFound`] if no policy exists with `policy_id`
    /// - [`InsuranceError::Unauthorized`] if `caller` is neither the policy owner
    ///   nor the contract owner
    /// - [`InsuranceError::PolicyAlreadyInactive`] if the policy is already inactive
    pub fn deactivate_policy(
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
        if !policy.active {
            return Err(InsuranceError::PolicyAlreadyInactive);
        }

        policy.active = false;
        env.storage().instance().set(&DataKey::Policy(policy_id), &policy);
        // Remove from active index (helper)
        Self::remove_active_policy(&env, policy_id)?;

        env.events().publish(
            (symbol_short!("deactive"), symbol_short!("policy")),
            PolicyDeactivatedEvent {
                policy_id,
                name: policy.name,
                timestamp: env.ledger().timestamp(),
            },
        );
        Ok(true)
    }

    /// Reactivate a previously deactivated policy.
    ///
    /// Authorization: callable by the policy owner or contract owner.
    /// Reactivation sets `active = true`, refreshes `next_payment_date` and
    /// re-inserts the policy ID into the `ActivePolicies` index without
    /// duplicating an existing entry. If the active index is full this
    /// returns `MaxPoliciesReached`.
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

        // Refresh payment cadence to the next logical due date relative to now.
        let now = env.ledger().timestamp();
        policy.next_payment_date = Self::advance_next_payment_date(policy.next_payment_date, now);
        policy.active = true;
        env.storage().instance().set(&DataKey::Policy(policy_id), &policy);

        // Attempt to add to the active index; helper enforces capacity/dup.
        Self::add_active_policy(&env, policy_id)?;

        env.events().publish(
            (Symbol::new(&env, "reactivated"), symbol_short!("policy")),
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

        let mut items = Vec::new(&env);
        let mut next_cursor = 0u32;

        let lim = if limit == 0 {
            DEFAULT_PAGE_LIMIT
        } else if limit > MAX_PAGE_LIMIT {
            MAX_PAGE_LIMIT
        } else {
            limit
        };

        let mut last_active_id = 0u32;
        for id in owner_ids.iter() {
            if id > cursor {
                if let Some(p) = env
                    .storage()
                    .instance()
                    .get::<_, Policy>(&DataKey::Policy(id))
                {
                    if p.active {
                        if items.len() < lim {
                            items.push_back(id);
                            last_active_id = id;
                        } else {
                            next_cursor = last_active_id;
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
    /// Mirrors the shape and semantics of `get_active_policies` but filters
    /// for policies where `active == false`. `limit` is normalized via
    /// `remitwise_common::clamp_limit`.
    pub fn get_deactivated_policies(
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

        let mut items = Vec::new(&env);
        let mut next_cursor = 0u32;

        let lim = remitwise_common::clamp_limit(limit);

        let mut last_inactive_id = 0u32;
        for id in owner_ids.iter() {
            if id > cursor {
                if let Some(p) = env
                    .storage()
                    .instance()
                    .get::<_, Policy>(&DataKey::Policy(id))
                {
                    if !p.active {
                        if items.len() < lim {
                            items.push_back(id);
                            last_inactive_id = id;
                        } else {
                            next_cursor = last_inactive_id;
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

    /// Get a policy by ID.
    ///
    /// # Errors
    /// - `NotInitialized` if the contract has not been initialized
    pub fn get_policy(
        env: Env,
        policy_id: u32,
    ) -> Result<core::option::Option<Policy>, InsuranceError> {
        Self::require_initialized(&env)?;

        Ok(env.storage().instance().get(&DataKey::Policy(policy_id)))
    }

    /// Get the total monthly premium for all active policies owned by an address.
    ///
    /// # Errors
    /// - `NotInitialized` if the contract has not been initialized
    pub fn get_total_monthly_premium(env: Env, owner: Address) -> Result<i128, InsuranceError> {
        Self::require_initialized(&env)?;

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

        Ok(total)
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
            .ok_or(InsuranceError::NotInitialized)?;
        if snapshot.schema_version != SNAPSHOT_VERSION {
            return Err(InsuranceError::Unauthorized);
        }
        if snapshot.owner != owner {
            return Err(InsuranceError::Unauthorized);
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
        env.storage()
            .persistent()
            .extend_ttl(key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
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
            (symbol_short!("insurance"), symbol_short!("sched_crt")),
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
            (symbol_short!("insurance"), symbol_short!("sched_mod")),
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
            (symbol_short!("insurance"), symbol_short!("sched_ccl")),
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
                (symbol_short!("insurance"), symbol_short!("sched_exe")),
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
mod test;
#[cfg(test)]
mod next_payment_scheduling_tests;
#[cfg(test)]
mod events_schema_test;
