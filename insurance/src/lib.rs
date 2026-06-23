#![no_std]
use remitwise_common::{
    CoverageType, DEFAULT_PAGE_LIMIT, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
    MAX_PAGE_LIMIT,
};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String, Vec,
};

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

const THIRTY_DAYS_SECS: u64 = 30 * 24 * 60 * 60;
const MAX_NAME_LEN: u32 = 64;
const MAX_EXT_REF_LEN: u32 = 128;
const MAX_POLICIES: u32 = 1_000;

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
    /// Returned by `deactivate_policy` when the target policy is already inactive.
    /// Distinct from `PolicyInactive` (which signals a caller trying to act *on*
    /// an inactive policy) — `PolicyAlreadyInactive` signals that the *deactivation
    /// itself* is a no-op because the policy was never active (or was already
    /// deactivated by a prior call).
    PolicyAlreadyInactive = 12,
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
#[derive(Clone)]
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

#[contracttype]
pub enum DataKey {
    Owner,
    PolicyCount,
    Policy(u32),
    ActivePolicies,
    OwnerPolicies(Address),
    Initialized,
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

    fn get_owner(env: &Env) -> Result<Address, InsuranceError> {
        env.storage()
            .instance()
            .get(&DataKey::Owner)
            .ok_or(InsuranceError::NotInitialized)
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

        let mut active = env
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

        env.storage()
            .instance()
            .set(&DataKey::Policy(next_id), &policy);
        env.storage()
            .instance()
            .set(&DataKey::PolicyCount, &next_id);
        active.push_back(next_id);
        env.storage()
            .instance()
            .set(&DataKey::ActivePolicies, &active);

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
        policy.next_payment_date = now + THIRTY_DAYS_SECS;

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
                policy.next_payment_date = now + THIRTY_DAYS_SECS;
                env.storage().instance().set(&DataKey::Policy(id), &policy);
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
        env.storage()
            .instance()
            .set(&DataKey::Policy(policy_id), &policy);

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

    /// Get a paginated list of active policies for an owner.
    pub fn get_active_policies(
        env: Env,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> Result<PolicyPage, InsuranceError> {
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

    /// Get a policy by ID.
    pub fn get_policy(
        env: Env,
        policy_id: u32,
    ) -> Result<core::option::Option<Policy>, InsuranceError> {
        Ok(env.storage().instance().get(&DataKey::Policy(policy_id)))
    }

    /// Get the total monthly premium for all active policies owned by an address.
    pub fn get_total_monthly_premium(env: Env, owner: Address) -> Result<i128, InsuranceError> {
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
}

#[cfg(test)]
mod test;