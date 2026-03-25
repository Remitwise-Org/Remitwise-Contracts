#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, contracterror, symbol_short,
    Address, Env, Map, String, Symbol, Vec,
};

// ============================================================================
// Module declarations
// ============================================================================
mod test;

// ============================================================================
// Constants
// ============================================================================

/// Instance storage bump amount (~30 days in ledger sequences at ~5s/ledger).
const INSTANCE_BUMP_AMOUNT: u32 = 518_400;

/// Instance lifetime threshold (~1 day). Re-bump when TTL drops below this.
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280;

/// Maximum number of active policies across the entire contract.
const MAX_POLICIES: u32 = 1_000;

/// Maximum name length in bytes.
const MAX_NAME_LEN: u32 = 64;

/// Maximum external reference length in bytes.
const MAX_EXT_REF_LEN: u32 = 128;

/// Seconds in 30 days (used for premium due-date advancement).
const THIRTY_DAYS: u64 = 30 * 24 * 60 * 60; // 2_592_000

/// Maximum page limit for paginated queries (also the max batch size).
const MAX_PAGE_LIMIT: u32 = 50;

/// Default page limit when caller supplies 0.
const DEFAULT_PAGE_LIMIT: u32 = 20;

/// Ratio guard multiplier: coverage ≤ premium × 12 × RATIO_CAP.
const RATIO_CAP: i128 = 500;

// ============================================================================
// Coverage-type range tables
// ============================================================================

/// Returns `(min_premium, max_premium, min_coverage, max_coverage)` for a
/// given `CoverageType`.
fn coverage_bounds(ct: &CoverageType) -> (i128, i128, i128, i128) {
    match ct {
        CoverageType::Health    => (1_000_000, 500_000_000, 10_000_000, 100_000_000_000),
        CoverageType::Life      => (500_000, 1_000_000_000, 50_000_000, 500_000_000_000),
        CoverageType::Property  => (2_000_000, 2_000_000_000, 100_000_000, 1_000_000_000_000),
        CoverageType::Auto      => (1_500_000, 750_000_000, 20_000_000, 200_000_000_000),
        CoverageType::Liability => (800_000, 400_000_000, 5_000_000, 50_000_000_000),
    }
}

// ============================================================================
// Types
// ============================================================================

/// Insurance coverage types — shared with `remitwise-common`.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CoverageType {
    Health = 1,
    Life = 2,
    Property = 3,
    Auto = 4,
    Liability = 5,
}

/// Error codes surfaced by the insurance contract.
///
/// NatSpec: Each variant maps to a deterministic on-chain error code so that
/// off-chain clients can programmatically handle failures.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum InsuranceError {
    /// Caller is not authorized for this operation.
    Unauthorized = 1,
    /// Contract `init` has already been called.
    AlreadyInitialized = 2,
    /// Contract has not been initialized yet.
    NotInitialized = 3,
    /// The requested policy ID does not exist in storage.
    PolicyNotFound = 4,
    /// The policy is inactive or already deactivated.
    PolicyInactive = 5,
    /// The policy name is empty or exceeds the max length.
    InvalidName = 6,
    /// The monthly premium is non-positive or outside the allowed range.
    InvalidPremium = 7,
    /// The coverage amount is non-positive or outside the allowed range.
    InvalidCoverageAmount = 8,
    /// Coverage amount violates the ratio guard relative to the premium.
    UnsupportedCombination = 9,
    /// External reference exceeds the max length.
    InvalidExternalRef = 10,
    /// Global maximum number of active policies has been reached.
    MaxPoliciesReached = 11,
    /// The contract or the specific function is currently paused.
    ContractPaused = 12,
}

/// Data keys for contract instance storage.
///
/// NatSpec: All durable state is stored in the instance bucket so that a single
/// TTL bump covers the entire contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// The contract owner address (set once via `init`).
    Owner,
    /// Monotonically increasing policy-ID counter.
    PolicyCount,
    /// Map from policy ID → `InsurancePolicy`.
    Policies,
    /// Vec of IDs that are currently active (used for iteration).
    ActivePolicies,
    /// Global emergency-pause flag (bool). When `true`, ALL mutators are
    /// blocked.
    PauseAll,
    /// Per-function pause flags stored as a `Map<Symbol, bool>`.
    /// Supported keys: `"create"`, `"pay"`, `"deactivate"`, `"set_ref"`,
    /// `"schedule"`.
    PauseFn,
    /// Monotonically increasing schedule-ID counter.
    ScheduleCount,
    /// Map from schedule ID → `PremiumSchedule`.
    Schedules,
}

/// A single insurance policy record.
///
/// NatSpec: Policies are stored in an instance-scoped map keyed by a `u32` ID
/// that starts at 1 and increments monotonically on each `create_policy` call.
#[contracttype]
#[derive(Clone, Debug)]
pub struct InsurancePolicy {
    /// Unique numeric identifier (starts at 1).
    pub id: u32,
    /// Address of the policyholder who created this policy.
    pub owner: Address,
    /// Human-readable label (1–64 bytes).
    pub name: String,
    /// Coverage category (Health, Life, Property, Auto, Liability).
    pub coverage_type: CoverageType,
    /// Monthly cost in stroops.
    pub monthly_premium: i128,
    /// Total insured value in stroops.
    pub coverage_amount: i128,
    /// Whether the policy is still active.
    pub active: bool,
    /// Ledger timestamp of the most recent premium payment (0 if never paid).
    pub last_payment_at: u64,
    /// Ledger timestamp when the next premium is due.
    pub next_payment_due: u64,
    /// Ledger timestamp when the policy was created.
    pub created_at: u64,
    /// Optional off-chain reference string (1–128 bytes, or None).
    pub external_ref: Option<String>,
    /// Alias kept for backward-compat with some older test helpers.
    pub next_payment_date: u64,
}

/// Paginated result set for policy queries.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PolicyPage {
    /// The policies on this page.
    pub items: Vec<InsurancePolicy>,
    /// Number of items on this page.
    pub count: u32,
    /// Cursor value for the next page (0 means no more pages).
    pub next_cursor: u32,
}

/// A scheduled premium-payment entry.
///
/// NatSpec: Schedules allow automated or batched premium payments at fixed
/// intervals. They are stored in instance storage alongside policies.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PremiumSchedule {
    pub id: u32,
    pub policy_id: u32,
    pub owner: Address,
    pub next_due: u64,
    pub interval: u64,
    pub active: bool,
    pub missed_count: u32,
}

// ============================================================================
// Contract
// ============================================================================

/// The RemitWise Insurance smart contract.
///
/// NatSpec: This contract manages micro-insurance policies for RemitWise users.
/// It enforces strict per-coverage-type validation, owner-only administrative
/// operations, and supports both global emergency pause and granular
/// per-function pause controls.
#[contract]
pub struct Insurance;

#[contractimpl]
impl Insurance {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    /// Initialize the insurance contract.
    ///
    /// NatSpec: Must be called exactly once. Sets the contract owner, resets the
    /// policy counter to 0, and initializes the active-policy list to empty.
    /// Panics with `"already initialized"` on subsequent calls.
    ///
    /// # Arguments
    /// * `owner` — The address that will serve as the contract administrator.
    ///
    /// # Security
    /// The owner address is immutable after initialization.
    pub fn init(env: Env, owner: Address) {
        if env.storage().instance().has(&DataKey::Owner) {
            panic!("already initialized");
        }
        owner.require_auth();

        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage().instance().set(&DataKey::PolicyCount, &0u32);

        let empty_policies: Map<u32, InsurancePolicy> = Map::new(&env);
        env.storage().instance().set(&DataKey::Policies, &empty_policies);

        let empty_active: Vec<u32> = Vec::new(&env);
        env.storage().instance().set(&DataKey::ActivePolicies, &empty_active);

        env.storage().instance().set(&DataKey::PauseAll, &false);

        let empty_pause_fn: Map<Symbol, bool> = Map::new(&env);
        env.storage().instance().set(&DataKey::PauseFn, &empty_pause_fn);

        env.storage().instance().set(&DataKey::ScheduleCount, &0u32);
        let empty_schedules: Map<u32, PremiumSchedule> = Map::new(&env);
        env.storage().instance().set(&DataKey::Schedules, &empty_schedules);

        Self::bump_ttl(&env);
    }

    // -----------------------------------------------------------------------
    // Pause controls (owner-only)
    // -----------------------------------------------------------------------

    /// Set or clear the global emergency pause flag.
    ///
    /// NatSpec: When `paused` is `true`, ALL state-mutating functions will
    /// panic with `"contract is paused"`. Only the contract owner may toggle
    /// this flag.
    ///
    /// # Security
    /// * Requires `owner.require_auth()`.
    /// * Does NOT require the contract to be un-paused — the owner can always
    ///   toggle this flag.
    pub fn set_pause_all(env: Env, owner: Address, paused: bool) {
        Self::require_owner(&env, &owner);
        env.storage().instance().set(&DataKey::PauseAll, &paused);
        Self::bump_ttl(&env);
    }

    /// Set or clear a per-function pause flag.
    ///
    /// NatSpec: Supported `fn_name` values:
    /// `"create"`, `"pay"`, `"deactivate"`, `"set_ref"`, `"schedule"`.
    ///
    /// When paused, only the corresponding function is blocked; others remain
    /// available.
    ///
    /// # Security
    /// * Requires `owner.require_auth()`.
    pub fn set_pause_fn(env: Env, owner: Address, fn_name: Symbol, paused: bool) {
        Self::require_owner(&env, &owner);
        let mut pause_map: Map<Symbol, bool> = env
            .storage()
            .instance()
            .get(&DataKey::PauseFn)
            .unwrap_or(Map::new(&env));
        pause_map.set(fn_name, paused);
        env.storage().instance().set(&DataKey::PauseFn, &pause_map);
        Self::bump_ttl(&env);
    }

    /// Query whether the global pause flag is set.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::PauseAll)
            .unwrap_or(false)
    }

    /// Query whether a specific function is paused (either globally or
    /// per-function).
    pub fn is_fn_paused(env: Env, fn_name: Symbol) -> bool {
        let global: bool = env
            .storage()
            .instance()
            .get(&DataKey::PauseAll)
            .unwrap_or(false);
        if global {
            return true;
        }
        let pause_map: Map<Symbol, bool> = env
            .storage()
            .instance()
            .get(&DataKey::PauseFn)
            .unwrap_or(Map::new(&env));
        pause_map.get(fn_name).unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // Policy management
    // -----------------------------------------------------------------------

    /// Create a new insurance policy.
    ///
    /// NatSpec: Validates all inputs against coverage-type bounds, ratio guard,
    /// name/external-ref lengths, and capacity limits. Emits a
    /// `PolicyCreatedEvent` on success.
    ///
    /// # Panics
    /// * `"not initialized"` — contract has not been initialized.
    /// * `"contract is paused"` / `"create is paused"` — pause controls active.
    /// * `"name cannot be empty"` / `"name too long"` — name validation.
    /// * `"monthly_premium must be positive"` / `"monthly_premium out of range
    ///    for coverage type"` — premium validation.
    /// * `"coverage_amount must be positive"` / `"coverage_amount out of range
    ///    for coverage type"` — coverage validation.
    /// * `"unsupported combination: coverage_amount too high relative to
    ///    premium"` — ratio guard.
    /// * `"external_ref length out of range"` — external ref validation.
    /// * `"max policies reached"` — capacity limit.
    ///
    /// # Returns
    /// The new policy's `u32` ID.
    pub fn create_policy(
        env: Env,
        caller: Address,
        name: String,
        coverage_type: CoverageType,
        monthly_premium: i128,
        coverage_amount: i128,
        external_ref: Option<String>,
    ) -> u32 {
        Self::require_init(&env);
        Self::require_not_paused(&env, "create");
        caller.require_auth();

        // --- Name validation ---
        let name_len = name.len();
        if name_len == 0 {
            panic!("name cannot be empty");
        }
        if name_len > MAX_NAME_LEN {
            panic!("name too long");
        }

        // --- Premium validation ---
        if monthly_premium <= 0 {
            panic!("monthly_premium must be positive");
        }
        let (min_p, max_p, min_c, max_c) = coverage_bounds(&coverage_type);
        if monthly_premium < min_p || monthly_premium > max_p {
            panic!("monthly_premium out of range for coverage type");
        }

        // --- Coverage validation ---
        if coverage_amount <= 0 {
            panic!("coverage_amount must be positive");
        }
        if coverage_amount < min_c || coverage_amount > max_c {
            panic!("coverage_amount out of range for coverage type");
        }

        // --- Ratio guard ---
        let max_coverage = monthly_premium
            .checked_mul(12)
            .expect("overflow")
            .checked_mul(RATIO_CAP)
            .expect("overflow");
        if coverage_amount > max_coverage {
            panic!("unsupported combination: coverage_amount too high relative to premium");
        }

        // --- External ref validation ---
        if let Some(ref ext) = external_ref {
            let ext_len = ext.len();
            if ext_len == 0 || ext_len > MAX_EXT_REF_LEN {
                panic!("external_ref length out of range");
            }
        }

        // --- Capacity check ---
        let active_ids: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::ActivePolicies)
            .unwrap();
        if active_ids.len() >= MAX_POLICIES {
            panic!("max policies reached");
        }

        // --- Allocate ID ---
        let mut count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PolicyCount)
            .unwrap();
        count = count.checked_add(1).expect("policy ID overflow");

        let now = env.ledger().timestamp();
        let next_due = now + THIRTY_DAYS;

        let policy = InsurancePolicy {
            id: count,
            owner: caller.clone(),
            name: name.clone(),
            coverage_type,
            monthly_premium,
            coverage_amount,
            active: true,
            last_payment_at: 0,
            next_payment_due: next_due,
            created_at: now,
            external_ref,
            next_payment_date: next_due,
        };

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DataKey::Policies)
            .unwrap();
        policies.set(count, policy);
        env.storage().instance().set(&DataKey::Policies, &policies);
        env.storage().instance().set(&DataKey::PolicyCount, &count);

        let mut active: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::ActivePolicies)
            .unwrap();
        active.push_back(count);
        env.storage().instance().set(&DataKey::ActivePolicies, &active);

        // Emit event
        env.events().publish(
            (symbol_short!("created"), symbol_short!("policy")),
            (count, now),
        );

        Self::bump_ttl(&env);
        count
    }

    /// Record a premium payment for a policy.
    ///
    /// NatSpec: Advances `next_payment_due` by 30 days from the current ledger
    /// timestamp and updates `last_payment_at`. Panics if the policy is
    /// inactive, nonexistent, or the contract is paused.
    ///
    /// # Returns
    /// `true` on success.
    pub fn pay_premium(env: Env, caller: Address, policy_id: u32) -> bool {
        Self::require_init(&env);
        Self::require_not_paused(&env, "pay");
        caller.require_auth();

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DataKey::Policies)
            .unwrap();

        let mut policy = match policies.get(policy_id) {
            Some(p) => p,
            None => panic!("policy not found"),
        };

        if !policy.active {
            panic!("policy inactive");
        }
        if policy.owner != caller {
            panic!("Only the policy owner can pay premiums");
        }

        let now = env.ledger().timestamp();
        let next = now + THIRTY_DAYS;
        policy.last_payment_at = now;
        policy.next_payment_due = next;
        policy.next_payment_date = next;

        policies.set(policy_id, policy);
        env.storage().instance().set(&DataKey::Policies, &policies);

        env.events().publish(
            (symbol_short!("paid"), symbol_short!("premium")),
            (policy_id, now),
        );

        Self::bump_ttl(&env);
        true
    }

    /// Owner-only: update or clear the `external_ref` field of a policy.
    ///
    /// NatSpec: Validates the new external reference length if provided. Only
    /// the contract owner may call this function.
    ///
    /// # Returns
    /// `true` on success.
    pub fn set_external_ref(
        env: Env,
        owner: Address,
        policy_id: u32,
        ext_ref: Option<String>,
    ) -> bool {
        Self::require_init(&env);
        Self::require_not_paused(&env, "set_ref");
        Self::require_owner(&env, &owner);

        if let Some(ref e) = ext_ref {
            let elen = e.len();
            if elen == 0 || elen > MAX_EXT_REF_LEN {
                panic!("external_ref length out of range");
            }
        }

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DataKey::Policies)
            .unwrap();
        let mut policy = match policies.get(policy_id) {
            Some(p) => p,
            None => panic!("policy not found"),
        };

        policy.external_ref = ext_ref;
        policies.set(policy_id, policy);
        env.storage().instance().set(&DataKey::Policies, &policies);

        Self::bump_ttl(&env);
        true
    }

    /// Owner-only: deactivate a policy.
    ///
    /// NatSpec: Marks the policy as inactive and removes it from the active-ID
    /// list. Emits a `PolicyDeactivatedEvent`. Panics if already inactive.
    ///
    /// # Returns
    /// `true` on success.
    pub fn deactivate_policy(env: Env, owner: Address, policy_id: u32) -> bool {
        Self::require_init(&env);
        Self::require_not_paused(&env, "deactivate");
        Self::require_owner(&env, &owner);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DataKey::Policies)
            .unwrap();
        let mut policy = match policies.get(policy_id) {
            Some(p) => p,
            None => panic!("policy not found"),
        };

        if !policy.active {
            panic!("policy already inactive");
        }

        policy.active = false;
        policies.set(policy_id, policy);
        env.storage().instance().set(&DataKey::Policies, &policies);

        // Remove from active list
        let active: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::ActivePolicies)
            .unwrap();
        let mut new_active = Vec::new(&env);
        for aid in active.iter() {
            if aid != policy_id {
                new_active.push_back(aid);
            }
        }
        env.storage().instance().set(&DataKey::ActivePolicies, &new_active);

        env.events().publish(
            (symbol_short!("deactive"), symbol_short!("policy")),
            (policy_id, env.ledger().timestamp()),
        );

        Self::bump_ttl(&env);
        true
    }

    /// Batch premium payment for multiple policies in one call.
    ///
    /// NatSpec: Processes up to `MAX_BATCH_SIZE` (50) policies. Each policy
    /// must be active and owned by the caller. Non-matching policies are
    /// silently skipped.
    ///
    /// # Returns
    /// The number of policies successfully paid.
    pub fn batch_pay_premiums(env: Env, caller: Address, policy_ids: Vec<u32>) -> u32 {
        Self::require_init(&env);
        Self::require_not_paused(&env, "pay");
        caller.require_auth();

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DataKey::Policies)
            .unwrap();

        let now = env.ledger().timestamp();
        let next = now + THIRTY_DAYS;
        let mut paid = 0u32;

        for pid in policy_ids.iter() {
            if let Some(mut policy) = policies.get(pid) {
                if policy.active && policy.owner == caller {
                    policy.last_payment_at = now;
                    policy.next_payment_due = next;
                    policy.next_payment_date = next;
                    policies.set(pid, policy);
                    paid += 1;
                }
            }
        }

        env.storage().instance().set(&DataKey::Policies, &policies);
        Self::bump_ttl(&env);
        paid
    }

    // -----------------------------------------------------------------------
    // Premium schedules
    // -----------------------------------------------------------------------

    /// Create a new premium schedule for a policy.
    ///
    /// NatSpec: Schedules allow automated recurring payments. The `next_due`
    /// timestamp is set by the caller. If `interval` is 0, the schedule fires
    /// once and then deactivates.
    ///
    /// # Returns
    /// The new schedule's `u32` ID.
    pub fn create_premium_schedule(
        env: Env,
        caller: Address,
        policy_id: u32,
        next_due: u64,
        interval: u64,
    ) -> u32 {
        Self::require_init(&env);
        Self::require_not_paused(&env, "schedule");
        caller.require_auth();

        let mut scount: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ScheduleCount)
            .unwrap();
        scount = scount.checked_add(1).expect("schedule ID overflow");

        let schedule = PremiumSchedule {
            id: scount,
            policy_id,
            owner: caller,
            next_due,
            interval,
            active: true,
            missed_count: 0,
        };

        let mut schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&DataKey::Schedules)
            .unwrap();
        schedules.set(scount, schedule);
        env.storage().instance().set(&DataKey::Schedules, &schedules);
        env.storage().instance().set(&DataKey::ScheduleCount, &scount);

        Self::bump_ttl(&env);
        scount
    }

    /// Modify an existing premium schedule's `next_due` and `interval`.
    ///
    /// NatSpec: Only the schedule owner may modify. Panics if the schedule does
    /// not exist.
    pub fn modify_premium_schedule(
        env: Env,
        caller: Address,
        schedule_id: u32,
        next_due: u64,
        interval: u64,
    ) {
        Self::require_init(&env);
        Self::require_not_paused(&env, "schedule");
        caller.require_auth();

        let mut schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&DataKey::Schedules)
            .unwrap();
        let mut schedule = schedules.get(schedule_id).expect("schedule not found");

        if schedule.owner != caller {
            panic!("unauthorized");
        }

        schedule.next_due = next_due;
        schedule.interval = interval;
        schedules.set(schedule_id, schedule);
        env.storage().instance().set(&DataKey::Schedules, &schedules);

        Self::bump_ttl(&env);
    }

    /// Cancel a premium schedule.
    ///
    /// NatSpec: Marks the schedule as inactive. Only the owner may cancel.
    pub fn cancel_premium_schedule(env: Env, caller: Address, schedule_id: u32) {
        Self::require_init(&env);
        Self::require_not_paused(&env, "schedule");
        caller.require_auth();

        let mut schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&DataKey::Schedules)
            .unwrap();
        let mut schedule = schedules.get(schedule_id).expect("schedule not found");

        if schedule.owner != caller {
            panic!("unauthorized");
        }

        schedule.active = false;
        schedules.set(schedule_id, schedule);
        env.storage().instance().set(&DataKey::Schedules, &schedules);

        Self::bump_ttl(&env);
    }

    /// Execute all premium schedules that are currently due.
    ///
    /// NatSpec: Iterates through all active schedules, executing those whose
    /// `next_due` ≤ current timestamp. For recurring schedules (`interval > 0`),
    /// the schedule advances. For one-shot schedules (`interval == 0`), the
    /// schedule deactivates. Missed intervals are counted.
    ///
    /// # Returns
    /// A `Vec<u32>` of schedule IDs that were executed.
    pub fn execute_due_premium_schedules(env: Env) -> Vec<u32> {
        Self::require_init(&env);

        let now = env.ledger().timestamp();
        let mut schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&DataKey::Schedules)
            .unwrap();
        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DataKey::Policies)
            .unwrap();

        let mut executed = Vec::new(&env);
        let scount: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ScheduleCount)
            .unwrap();

        for sid in 1..=scount {
            if let Some(mut schedule) = schedules.get(sid) {
                if !schedule.active || schedule.next_due > now {
                    continue;
                }

                // Execute the premium payment for the associated policy
                if let Some(mut policy) = policies.get(schedule.policy_id) {
                    if policy.active {
                        let next = now + THIRTY_DAYS;
                        policy.last_payment_at = now;
                        policy.next_payment_due = next;
                        policy.next_payment_date = next;
                        policies.set(schedule.policy_id, policy);
                    }
                }

                // Handle missed intervals for recurring schedules
                if schedule.interval > 0 {
                    let mut missed: u32 = 0;
                    let mut due = schedule.next_due;
                    while due + schedule.interval <= now {
                        due += schedule.interval;
                        missed += 1;
                    }
                    schedule.missed_count = missed;
                    schedule.next_due = due + schedule.interval;
                    // Stays active — recurring
                } else {
                    // One-shot schedule: deactivate after execution
                    schedule.active = false;
                }

                schedules.set(sid, schedule);
                executed.push_back(sid);
            }
        }

        env.storage().instance().set(&DataKey::Schedules, &schedules);
        env.storage().instance().set(&DataKey::Policies, &policies);

        Self::bump_ttl(&env);
        executed
    }

    /// Query a premium schedule by ID.
    pub fn get_premium_schedule(env: Env, schedule_id: u32) -> Option<PremiumSchedule> {
        let schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&DataKey::Schedules)
            .unwrap_or(Map::new(&env));
        schedules.get(schedule_id)
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Retrieve a policy by its ID.
    ///
    /// NatSpec: Panics with `"policy not found"` if the requested ID does not
    /// exist. Does not require authorization (read-only).
    pub fn get_policy(env: Env, policy_id: u32) -> InsurancePolicy {
        Self::require_init(&env);
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DataKey::Policies)
            .unwrap();
        match policies.get(policy_id) {
            Some(p) => p,
            None => panic!("policy not found"),
        }
    }

    /// Return the list of all active policy IDs (unpaginated).
    ///
    /// NatSpec: This is the simple version; for paginated access use
    /// `get_active_policies` with cursor and limit.
    pub fn get_active_policies_list(env: Env) -> Vec<u32> {
        Self::require_init(&env);
        env.storage()
            .instance()
            .get(&DataKey::ActivePolicies)
            .unwrap()
    }

    /// Return active policies for a specific owner with cursor-based
    /// pagination.
    ///
    /// NatSpec: Returns a `PolicyPage` containing up to `limit` policies
    /// belonging to `owner` whose IDs are greater than `cursor`. Use
    /// `next_cursor` from the returned page to request subsequent pages.
    pub fn get_active_policies(
        env: Env,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> PolicyPage {
        Self::require_init(&env);

        let effective_limit = if limit == 0 {
            DEFAULT_PAGE_LIMIT
        } else if limit > MAX_PAGE_LIMIT {
            MAX_PAGE_LIMIT
        } else {
            limit
        };

        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DataKey::Policies)
            .unwrap();
        let active_ids: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::ActivePolicies)
            .unwrap();

        let mut items = Vec::new(&env);
        let mut count = 0u32;
        let mut last_id = 0u32;

        for aid in active_ids.iter() {
            if aid <= cursor {
                continue;
            }
            if let Some(policy) = policies.get(aid) {
                if policy.owner == owner && policy.active {
                    items.push_back(policy);
                    count += 1;
                    last_id = aid;
                    if count >= effective_limit {
                        break;
                    }
                }
            }
        }

        let next_cursor = if count >= effective_limit { last_id } else { 0 };

        PolicyPage {
            items,
            count,
            next_cursor,
        }
    }

    /// Return ALL policies for a specific owner (active and inactive) with
    /// pagination.
    pub fn get_all_policies_for_owner(
        env: Env,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> PolicyPage {
        Self::require_init(&env);

        let effective_limit = if limit == 0 {
            DEFAULT_PAGE_LIMIT
        } else if limit > MAX_PAGE_LIMIT {
            MAX_PAGE_LIMIT
        } else {
            limit
        };

        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DataKey::Policies)
            .unwrap();
        let total_count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PolicyCount)
            .unwrap();

        let mut items = Vec::new(&env);
        let mut count = 0u32;
        let mut last_id = 0u32;

        for pid in (cursor + 1)..=(total_count) {
            if let Some(policy) = policies.get(pid) {
                if policy.owner == owner {
                    items.push_back(policy);
                    count += 1;
                    last_id = pid;
                    if count >= effective_limit {
                        break;
                    }
                }
            }
        }

        let next_cursor = if count >= effective_limit { last_id } else { 0 };

        PolicyPage {
            items,
            count,
            next_cursor,
        }
    }

    /// Compute the sum of `monthly_premium` across all active policies for an
    /// owner.
    ///
    /// NatSpec: Uses `saturating_add` to prevent overflow on extremely large
    /// portfolios. READ-ONLY — no authorization required.
    pub fn get_total_monthly_premium(env: Env, owner: Address) -> i128 {
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DataKey::Policies)
            .unwrap_or(Map::new(&env));
        let active_ids: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::ActivePolicies)
            .unwrap_or(Vec::new(&env));

        let mut total: i128 = 0;
        for aid in active_ids.iter() {
            if let Some(policy) = policies.get(aid) {
                if policy.owner == owner {
                    total = total.saturating_add(policy.monthly_premium);
                }
            }
        }
        total
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Ensure the contract has been initialized.
    fn require_init(env: &Env) {
        if !env.storage().instance().has(&DataKey::Owner) {
            panic!("not initialized");
        }
    }

    /// Ensure the caller is the contract owner.
    fn require_owner(env: &Env, caller: &Address) {
        Self::require_init(env);
        caller.require_auth();
        let owner: Address = env
            .storage()
            .instance()
            .get(&DataKey::Owner)
            .unwrap();
        if *caller != owner {
            panic!("unauthorized");
        }
    }

    /// Check global and per-function pause controls.
    ///
    /// NatSpec: Panics with `"contract is paused"` if the global flag is set,
    /// or `"<fn> is paused"` if the specific function pause flag is set.
    fn require_not_paused(env: &Env, fn_name: &str) {
        let global: bool = env
            .storage()
            .instance()
            .get(&DataKey::PauseAll)
            .unwrap_or(false);
        if global {
            panic!("contract is paused");
        }
        let pause_map: Map<Symbol, bool> = env
            .storage()
            .instance()
            .get(&DataKey::PauseFn)
            .unwrap_or(Map::new(env));
        let sym = Symbol::new(env, fn_name);
        if pause_map.get(sym).unwrap_or(false) {
            match fn_name {
                "create" => panic!("create is paused"),
                "pay" => panic!("pay is paused"),
                "deactivate" => panic!("deactivate is paused"),
                "set_ref" => panic!("set_ref is paused"),
                "schedule" => panic!("schedule is paused"),
                _ => panic!("function is paused"),
            }
        }
    }

    /// Bump instance TTL when it is below the threshold.
    fn bump_ttl(env: &Env) {
        env.storage().instance().extend_ttl(
            INSTANCE_LIFETIME_THRESHOLD,
            INSTANCE_BUMP_AMOUNT,
        );
    }
}