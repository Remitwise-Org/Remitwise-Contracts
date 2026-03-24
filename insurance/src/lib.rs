//! # Insurance Contract
//!
//! Manages micro-insurance policies and premium payments for the RemitWise platform.
//!
//! ## Overview
//!
//! This contract enforces strict validation on policy creation to ensure:
//! - Only supported coverage types are accepted
//! - Monthly premiums fall within valid numeric ranges
//! - Coverage amounts are within acceptable bounds
//! - Unsupported combinations of coverage type and amounts are rejected
//!
//! ## Security Model
//!
//! - All state-changing functions require caller authorization via `require_auth()`
//! - Only the contract owner can deactivate policies or set external references
//! - Policy IDs are monotonically incrementing u32 values (overflow-safe)
//! - All numeric inputs are validated before storage to prevent overflow/underflow
//!
//! ## Coverage Types and Constraints
//!
//! | Coverage Type | Min Premium (stroops) | Max Premium (stroops) | Min Coverage | Max Coverage |
//! |---------------|-----------------------|-----------------------|--------------|--------------|
//! | Health        | 1_000_000             | 500_000_000           | 10_000_000   | 100_000_000_000 |
//! | Life          | 500_000               | 1_000_000_000         | 50_000_000   | 500_000_000_000 |
//! | Property      | 2_000_000             | 2_000_000_000         | 100_000_000  | 1_000_000_000_000 |
//! | Auto          | 1_500_000             | 750_000_000           | 20_000_000   | 200_000_000_000 |
//! | Liability     | 800_000               | 400_000_000           | 5_000_000    | 50_000_000_000 |

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum name length for a policy (bytes).
const MAX_NAME_LEN: u32 = 64;

/// Maximum external reference length (bytes).
const MAX_EXT_REF_LEN: u32 = 128;

/// Maximum number of active policies per contract instance.
const MAX_POLICIES: u32 = 1_000;

// ---------------------------------------------------------------------------
// Coverage type enum
// ---------------------------------------------------------------------------

/// Supported insurance coverage types.
///
/// Each variant maps to a distinct set of premium and coverage-amount constraints.
/// Any value not matching one of these variants is rejected at policy creation time.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum CoverageType {
    /// Medical/healthcare coverage.
    Health,
    /// Term or whole-life coverage.
    Life,
    /// Residential or commercial property coverage.
    Property,
    /// Vehicle coverage.
    Auto,
    /// General liability coverage.
    Liability,
}

// ---------------------------------------------------------------------------
// Validation constraints per coverage type
// ---------------------------------------------------------------------------

/// Per-coverage-type numeric constraints (all values in stroops, 1 XLM = 10_000_000 stroops).
struct CoverageConstraints {
    min_premium: i128,
    max_premium: i128,
    min_coverage: i128,
    max_coverage: i128,
}

impl CoverageConstraints {
    /// Returns the constraints for the given coverage type.
    ///
    /// # Panics
    ///
    /// Never panics — every `CoverageType` variant has an entry.
    fn for_type(coverage_type: &CoverageType) -> Self {
        match coverage_type {
            CoverageType::Health => Self {
                min_premium: 1_000_000,
                max_premium: 500_000_000,
                min_coverage: 10_000_000,
                max_coverage: 100_000_000_000,
            },
            CoverageType::Life => Self {
                min_premium: 500_000,
                max_premium: 1_000_000_000,
                min_coverage: 50_000_000,
                max_coverage: 500_000_000_000,
            },
            CoverageType::Property => Self {
                min_premium: 2_000_000,
                max_premium: 2_000_000_000,
                min_coverage: 100_000_000,
                max_coverage: 1_000_000_000_000,
            },
            CoverageType::Auto => Self {
                min_premium: 1_500_000,
                max_premium: 750_000_000,
                min_coverage: 20_000_000,
                max_coverage: 200_000_000_000,
            },
            CoverageType::Liability => Self {
                min_premium: 800_000,
                max_premium: 400_000_000,
                min_coverage: 5_000_000,
                max_coverage: 50_000_000_000,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Storage key types
// ---------------------------------------------------------------------------

/// Top-level storage keys for the contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// The contract owner address.
    Owner,
    /// Global policy counter (u32).
    PolicyCount,
    /// Individual policy record keyed by its u32 ID.
    Policy(u32),
    /// List of all active policy IDs.
    ActivePolicies,
}

// ---------------------------------------------------------------------------
// Policy record
// ---------------------------------------------------------------------------

/// A single insurance policy stored on-chain.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Policy {
    /// Unique monotonic ID assigned at creation.
    pub id: u32,
    /// Human-readable policy name.
    pub name: String,
    /// The type of coverage this policy provides.
    pub coverage_type: CoverageType,
    /// Monthly premium in stroops.
    pub monthly_premium: i128,
    /// Total coverage amount in stroops.
    pub coverage_amount: i128,
    /// Ledger timestamp at which the policy was created.
    pub created_at: u64,
    /// Ledger timestamp of the last premium payment (0 if never paid).
    pub last_payment_at: u64,
    /// Expected next payment due timestamp (created_at + 30 days in seconds).
    pub next_payment_due: u64,
    /// Whether the policy is currently active.
    pub active: bool,
    /// Optional opaque external reference for off-chain linking (e.g. provider ID).
    pub external_ref: Option<String>,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted when a new insurance policy is successfully created.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PolicyCreatedEvent {
    pub policy_id: u32,
    pub name: String,
    pub coverage_type: CoverageType,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub timestamp: u64,
}

/// Emitted when a premium payment is recorded.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PremiumPaidEvent {
    pub policy_id: u32,
    pub name: String,
    pub amount: i128,
    pub next_payment_date: u64,
    pub timestamp: u64,
}

/// Emitted when a policy is deactivated.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PolicyDeactivatedEvent {
    pub policy_id: u32,
    pub name: String,
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

/// Contract-level error codes returned via `panic_with_error!` / direct panics.
///
/// Using a typed enum makes it easy for callers and off-chain tooling to
/// distinguish validation failures from other unexpected errors.
#[contracttype]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum InsuranceError {
    /// Caller is not the contract owner.
    Unauthorized = 1,
    /// The contract has already been initialized.
    AlreadyInitialized = 2,
    /// The contract has not been initialized yet.
    NotInitialized = 3,
    /// The supplied policy ID does not exist.
    PolicyNotFound = 4,
    /// The policy has already been deactivated.
    PolicyInactive = 5,
    /// Policy name is empty or exceeds `MAX_NAME_LEN`.
    InvalidName = 6,
    /// Monthly premium is outside the allowed range for this coverage type.
    InvalidPremium = 7,
    /// Coverage amount is outside the allowed range for this coverage type.
    InvalidCoverageAmount = 8,
    /// The combination of coverage type and supplied amounts is not supported.
    UnsupportedCombination = 9,
    /// External reference exceeds `MAX_EXT_REF_LEN`.
    InvalidExternalRef = 10,
    /// Maximum number of active policies reached.
    MaxPoliciesReached = 11,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct InsuranceContract;

#[contractimpl]
impl InsuranceContract {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initializes the contract with the given owner address.
    ///
    /// # Arguments
    ///
    /// * `owner` - The address that will have administrative privileges.
    ///
    /// # Errors
    ///
    /// Panics with [`InsuranceError::AlreadyInitialized`] if called more than once.
    pub fn init(env: Env, owner: Address) {
        if env.storage().instance().has(&DataKey::Owner) {
            panic!("already initialized");
        }
        owner.require_auth();
        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage()
            .instance()
            .set(&DataKey::PolicyCount, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::ActivePolicies, &Vec::<u32>::new(&env));
    }

    // -----------------------------------------------------------------------
    // Policy creation
    // -----------------------------------------------------------------------

    /// Creates a new insurance policy after running strict validation checks.
    ///
    /// ## Validation rules (enforced in order)
    ///
    /// 1. Contract must be initialized.
    /// 2. Caller must authenticate (`require_auth`).
    /// 3. `name` must be non-empty and at most [`MAX_NAME_LEN`] bytes.
    /// 4. `monthly_premium` must be strictly positive.
    /// 5. `coverage_amount` must be strictly positive.
    /// 6. `monthly_premium` must be within the range defined for `coverage_type`.
    /// 7. `coverage_amount` must be within the range defined for `coverage_type`.
    /// 8. The combination must pass the ratio guard:
    ///    `coverage_amount <= monthly_premium * 12 * 500` (max leverage 500× annual).
    /// 9. Active policy count must not exceed [`MAX_POLICIES`].
    /// 10. If `external_ref` is provided it must not exceed [`MAX_EXT_REF_LEN`] bytes.
    ///
    /// # Arguments
    ///
    /// * `caller`          - Address of the policyholder (must sign).
    /// * `name`            - Human-readable policy label.
    /// * `coverage_type`   - One of the supported [`CoverageType`] variants.
    /// * `monthly_premium` - Monthly cost in stroops (must be > 0).
    /// * `coverage_amount` - Total insured value in stroops (must be > 0).
    /// * `external_ref`    - Optional opaque string for off-chain linking.
    ///
    /// # Returns
    ///
    /// The newly-assigned policy ID (`u32`).
    ///
    /// # Errors
    ///
    /// Panics with a descriptive message corresponding to an [`InsuranceError`] variant.
    pub fn create_policy(
        env: Env,
        caller: Address,
        name: String,
        coverage_type: CoverageType,
        monthly_premium: i128,
        coverage_amount: i128,
        external_ref: Option<String>,
    ) -> u32 {
        // 1. Ensure initialized
        Self::assert_initialized(&env);

        // 2. Authorization
        caller.require_auth();

        // 3. Validate name
        Self::validate_name(&name);

        // 4-5. Basic positivity checks (coverage-type-agnostic)
        if monthly_premium <= 0 {
            panic!("monthly_premium must be positive");
        }
        if coverage_amount <= 0 {
            panic!("coverage_amount must be positive");
        }

        // 6-8. Type-specific range + ratio validation
        Self::validate_coverage_constraints(&coverage_type, monthly_premium, coverage_amount);

        // 9. Validate external ref length
        if let Some(ref r) = external_ref {
            if r.len() == 0 || r.len() > MAX_EXT_REF_LEN {
                panic!("external_ref length out of range");
            }
        }

        // 10. Capacity guard
        let active_ids: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::ActivePolicies)
            .unwrap_or(Vec::new(&env));
        if active_ids.len() >= MAX_POLICIES {
            panic!("max policies reached");
        }

        // Mint a new policy ID (overflow-safe: MAX_POLICIES << u32::MAX)
        let mut count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PolicyCount)
            .unwrap_or(0u32);
        count = count.checked_add(1).expect("policy id overflow");
        let policy_id = count;

        let now = env.ledger().timestamp();
        // Next payment due ≈ 30 days from creation (30 * 24 * 60 * 60 seconds)
        let next_payment_due = now.saturating_add(30 * 24 * 60 * 60);

        let policy = Policy {
            id: policy_id,
            name: name.clone(),
            coverage_type: coverage_type.clone(),
            monthly_premium,
            coverage_amount,
            created_at: now,
            last_payment_at: 0,
            next_payment_due,
            active: true,
            external_ref: external_ref.clone(),
        };

        // Persist
        env.storage()
            .instance()
            .set(&DataKey::PolicyCount, &count);
        env.storage()
            .instance()
            .set(&DataKey::Policy(policy_id), &policy);

        let mut ids = active_ids;
        ids.push_back(policy_id);
        env.storage()
            .instance()
            .set(&DataKey::ActivePolicies, &ids);

        // Emit event
        env.events().publish(
            (symbol_short!("created"), symbol_short!("policy")),
            PolicyCreatedEvent {
                policy_id,
                name,
                coverage_type,
                monthly_premium,
                coverage_amount,
                timestamp: now,
            },
        );

        policy_id
    }

    // -----------------------------------------------------------------------
    // Premium payment
    // -----------------------------------------------------------------------

    /// Records a premium payment against an active policy.
    ///
    /// The payment amount must equal the policy's `monthly_premium` exactly.
    ///
    /// # Arguments
    ///
    /// * `caller`    - Address of the policyholder (must sign).
    /// * `policy_id` - ID of the policy to pay.
    /// * `amount`    - Amount paid in stroops (must equal `monthly_premium`).
    ///
    /// # Errors
    ///
    /// Panics if the policy is not found, is inactive, or the amount is incorrect.
    pub fn pay_premium(env: Env, caller: Address, policy_id: u32, amount: i128) -> bool {
        Self::assert_initialized(&env);
        caller.require_auth();

        let mut policy: Policy = env
            .storage()
            .instance()
            .get(&DataKey::Policy(policy_id))
            .unwrap_or_else(|| panic!("policy not found"));

        if !policy.active {
            panic!("policy inactive");
        }
        if amount != policy.monthly_premium {
            panic!("amount must equal monthly_premium");
        }

        let now = env.ledger().timestamp();
        policy.last_payment_at = now;
        policy.next_payment_due = now.saturating_add(30 * 24 * 60 * 60);

        env.storage()
            .instance()
            .set(&DataKey::Policy(policy_id), &policy);

        env.events().publish(
            (symbol_short!("paid"), symbol_short!("premium")),
            PremiumPaidEvent {
                policy_id,
                name: policy.name,
                amount,
                next_payment_date: policy.next_payment_due,
                timestamp: now,
            },
        );

        true
    }

    // -----------------------------------------------------------------------
    // External ref management
    // -----------------------------------------------------------------------

    /// Updates or clears the external reference for a policy.
    ///
    /// Only the contract owner may call this function.
    ///
    /// # Arguments
    ///
    /// * `owner`     - Must be the contract owner (will be auth-checked).
    /// * `policy_id` - Target policy.
    /// * `ext_ref`   - New reference value, or `None` to clear.
    pub fn set_external_ref(
        env: Env,
        owner: Address,
        policy_id: u32,
        ext_ref: Option<String>,
    ) -> bool {
        Self::assert_initialized(&env);
        Self::assert_owner(&env, &owner);
        owner.require_auth();

        if let Some(ref r) = ext_ref {
            if r.len() == 0 || r.len() > MAX_EXT_REF_LEN {
                panic!("external_ref length out of range");
            }
        }

        let mut policy: Policy = env
            .storage()
            .instance()
            .get(&DataKey::Policy(policy_id))
            .unwrap_or_else(|| panic!("policy not found"));

        policy.external_ref = ext_ref;
        env.storage()
            .instance()
            .set(&DataKey::Policy(policy_id), &policy);

        true
    }

    // -----------------------------------------------------------------------
    // Deactivation
    // -----------------------------------------------------------------------

    /// Deactivates an active policy.
    ///
    /// Only the contract owner may deactivate a policy.
    ///
    /// # Arguments
    ///
    /// * `owner`     - Must be the contract owner.
    /// * `policy_id` - Target policy.
    pub fn deactivate_policy(env: Env, owner: Address, policy_id: u32) -> bool {
        Self::assert_initialized(&env);
        Self::assert_owner(&env, &owner);
        owner.require_auth();

        let mut policy: Policy = env
            .storage()
            .instance()
            .get(&DataKey::Policy(policy_id))
            .unwrap_or_else(|| panic!("policy not found"));

        if !policy.active {
            panic!("policy already inactive");
        }

        policy.active = false;
        env.storage()
            .instance()
            .set(&DataKey::Policy(policy_id), &policy);

        // Remove from active list
        let active_ids: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::ActivePolicies)
            .unwrap_or(Vec::new(&env));
        let mut new_ids: Vec<u32> = Vec::new(&env);
        for id in active_ids.iter() {
            if id != policy_id {
                new_ids.push_back(id);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::ActivePolicies, &new_ids);

        let now = env.ledger().timestamp();
        env.events().publish(
            (symbol_short!("deactive"), symbol_short!("policy")),
            PolicyDeactivatedEvent {
                policy_id,
                name: policy.name,
                timestamp: now,
            },
        );

        true
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Returns all active policy IDs.
    pub fn get_active_policies(env: Env) -> Vec<u32> {
        Self::assert_initialized(&env);
        env.storage()
            .instance()
            .get(&DataKey::ActivePolicies)
            .unwrap_or(Vec::new(&env))
    }

    /// Returns the full policy record for `policy_id`.
    ///
    /// Panics if the policy does not exist.
    pub fn get_policy(env: Env, policy_id: u32) -> Policy {
        Self::assert_initialized(&env);
        env.storage()
            .instance()
            .get(&DataKey::Policy(policy_id))
            .unwrap_or_else(|| panic!("policy not found"))
    }

    /// Calculates the total monthly premium across all active policies.
    pub fn get_total_monthly_premium(env: Env) -> i128 {
        Self::assert_initialized(&env);
        let active_ids: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::ActivePolicies)
            .unwrap_or(Vec::new(&env));

        let mut total: i128 = 0;
        for id in active_ids.iter() {
            let policy: Policy = env
                .storage()
                .instance()
                .get(&DataKey::Policy(id))
                .unwrap_or_else(|| panic!("policy not found"));
            total = total.saturating_add(policy.monthly_premium);
        }
        total
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Panics if the contract has not been initialized.
    fn assert_initialized(env: &Env) {
        if !env.storage().instance().has(&DataKey::Owner) {
            panic!("not initialized");
        }
    }

    /// Panics if `addr` is not the stored owner.
    fn assert_owner(env: &Env, addr: &Address) {
        let owner: Address = env
            .storage()
            .instance()
            .get(&DataKey::Owner)
            .expect("not initialized");
        if owner != *addr {
            panic!("unauthorized");
        }
    }

    /// Validates the policy name length.
    fn validate_name(name: &String) {
        if name.len() == 0 {
            panic!("name cannot be empty");
        }
        if name.len() > MAX_NAME_LEN {
            panic!("name too long");
        }
    }

    /// Validates that `monthly_premium` and `coverage_amount` satisfy the

    /// per-coverage-type range constraints AND the premium-to-coverage ratio guard.
    ///
    /// ## Ratio guard
    ///
    /// To reject economically implausible policies, we require:
    ///
    /// ```text
    /// coverage_amount <= monthly_premium * 12 * 500
    /// ```
    ///
    /// This limits the leverage to 500× annual premium — far beyond any real-world
    /// micro-insurance product but low enough to prevent obviously nonsensical inputs.
    ///
    /// # Panics
    ///
    /// Panics with one of:
    /// - `"monthly_premium out of range for coverage type"`
    /// - `"coverage_amount out of range for coverage type"`
    /// - `"unsupported combination: coverage_amount too high relative to premium"`
    fn validate_coverage_constraints(
        coverage_type: &CoverageType,
        monthly_premium: i128,
        coverage_amount: i128,
    ) {
        let c = CoverageConstraints::for_type(coverage_type);

        // 6. Premium range
        if monthly_premium < c.min_premium || monthly_premium > c.max_premium {
            panic!("monthly_premium out of range for coverage type");
        }

        // 7. Coverage amount range
        if coverage_amount < c.min_coverage || coverage_amount > c.max_coverage {
            panic!("coverage_amount out of range for coverage type");
        }

        // 8. Ratio guard: coverage_amount <= premium * 12 * 500
        // Use checked arithmetic to avoid overflow (both values fit comfortably in i128)
        let annual_premium = monthly_premium
            .checked_mul(12)
            .expect("premium overflow in ratio check");
        let max_coverage_for_premium = annual_premium
            .checked_mul(500)
            .expect("ratio overflow in check");

        if coverage_amount > max_coverage_for_premium {
            panic!("unsupported combination: coverage_amount too high relative to premium");
        }
    }
}

mod test;