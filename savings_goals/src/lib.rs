#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, String,
    Symbol, Vec,
};
use remitwise_common::{EventCategory, EventPriority, RemitwiseEvents};

// Event topics
const GOAL_COMPLETED: Symbol = symbol_short!("completed");

#[derive(Clone)]
#[contracttype]
pub struct GoalCreatedEvent {
    pub goal_id: u32,
    pub name: String,
    pub target_amount: i128,
    pub target_date: u64,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct FundsAddedEvent {
    pub goal_id: u32,
    pub amount: i128,
    pub new_total: i128,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct GoalCompletedEvent {
    pub goal_id: u32,
    pub name: String,
    pub final_amount: i128,
    pub timestamp: u64,
}

const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280;
const INSTANCE_BUMP_AMOUNT: u32 = 518400;

/// Pagination constants
pub const DEFAULT_PAGE_LIMIT: u32 = 20;
pub const MAX_PAGE_LIMIT: u32 = 50;

#[contracttype]
#[derive(Clone)]
pub struct SavingsGoal {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub target_amount: i128,
    pub current_amount: i128,
    pub target_date: u64,
    pub locked: bool,
    pub unlock_date: Option<u64>,
    pub tags: Vec<String>,
}

/// Paginated result for savings goal queries
#[contracttype]
#[derive(Clone)]
pub struct GoalPage {
    /// Goals for this page
    pub items: Vec<SavingsGoal>,
    /// Pass as `cursor` for the next page. 0 = no more pages.
    pub next_cursor: u32,
    /// Number of items returned
    pub count: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct SavingsSchedule {
    pub id: u32,
    pub owner: Address,
    pub goal_id: u32,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavingsGoalsError {
    InvalidAmount = 1,
    GoalNotFound = 2,
    Unauthorized = 3,
    GoalLocked = 4,
    InsufficientBalance = 5,
    Overflow = 6,
}

impl From<SavingsGoalsError> for soroban_sdk::Error {
    fn from(err: SavingsGoalsError) -> Self {
        match err {
            SavingsGoalsError::InvalidAmount => soroban_sdk::Error::from((
                soroban_sdk::xdr::ScErrorType::Contract,
                soroban_sdk::xdr::ScErrorCode::InvalidInput,
            )),
            SavingsGoalsError::GoalNotFound => soroban_sdk::Error::from((
                soroban_sdk::xdr::ScErrorType::Contract,
                soroban_sdk::xdr::ScErrorCode::MissingValue,
            )),
            SavingsGoalsError::Unauthorized => soroban_sdk::Error::from((
                soroban_sdk::xdr::ScErrorType::Contract,
                soroban_sdk::xdr::ScErrorCode::InvalidAction,
            )),
            SavingsGoalsError::GoalLocked => soroban_sdk::Error::from((
                soroban_sdk::xdr::ScErrorType::Contract,
                soroban_sdk::xdr::ScErrorCode::InvalidAction,
            )),
            SavingsGoalsError::InsufficientBalance => soroban_sdk::Error::from((
                soroban_sdk::xdr::ScErrorType::Contract,
                soroban_sdk::xdr::ScErrorCode::InvalidInput,
            )),
            SavingsGoalsError::Overflow => soroban_sdk::Error::from((
                soroban_sdk::xdr::ScErrorType::Contract,
                soroban_sdk::xdr::ScErrorCode::InvalidInput,
            )),
        }
    }
}

impl From<&SavingsGoalsError> for soroban_sdk::Error {
    fn from(err: &SavingsGoalsError) -> Self {
        (*err).into()
    }
}

impl From<soroban_sdk::Error> for SavingsGoalsError {
    fn from(_err: soroban_sdk::Error) -> Self {
        SavingsGoalsError::Unauthorized
    }
}

#[contracttype]
#[derive(Clone)]
pub enum SavingsEvent {
    GoalCreated,
    FundsAdded,
    FundsWithdrawn,
    GoalCompleted,
    GoalLocked,
    GoalUnlocked,
    ScheduleCreated,
    ScheduleExecuted,
    ScheduleMissed,
    ScheduleModified,
    ScheduleCancelled,
}

/// Snapshot for savings goals export/import (migration).
///
/// # Schema Version Tag
/// `schema_version` carries the explicit snapshot format version.
/// Importers **must** validate this field against the supported range
/// (`MIN_SUPPORTED_SCHEMA_VERSION..=SCHEMA_VERSION`) before applying the
/// snapshot. Snapshots with an unknown future version must be rejected.
#[contracttype]
#[derive(Clone)]
pub struct GoalsExportSnapshot {
    /// Explicit schema version tag for this snapshot format.
    /// Supported range: MIN_SUPPORTED_SCHEMA_VERSION..=SCHEMA_VERSION.
    pub schema_version: u32,
    pub checksum: u64,
    pub next_id: u32,
    pub goals: Vec<SavingsGoal>,
}

#[contracttype]
#[derive(Clone)]
pub struct AuditEntry {
    pub operation: Symbol,
    pub caller: Address,
    pub timestamp: u64,
    pub success: bool,
}

/// Current snapshot schema version. Bump this when GoalsExportSnapshot format changes.
const SCHEMA_VERSION: u32 = 1;
/// Oldest snapshot schema version this contract can import. Enables backward compat.
const MIN_SUPPORTED_SCHEMA_VERSION: u32 = 1;
const MAX_AUDIT_ENTRIES: u32 = 100;
const CONTRACT_VERSION: u32 = 1;
const MAX_BATCH_SIZE: u32 = 50;

pub mod pause_functions {
    use soroban_sdk::{symbol_short, Symbol};
    pub const CREATE_GOAL: Symbol = symbol_short!("crt_goal");
    pub const ADD_TO_GOAL: Symbol = symbol_short!("add_goal");
    pub const WITHDRAW: Symbol = symbol_short!("withdraw");
    pub const LOCK: Symbol = symbol_short!("lock");
    pub const UNLOCK: Symbol = symbol_short!("unlock");
}

#[contracttype]
#[derive(Clone)]
pub struct ContributionItem {
    pub goal_id: u32,
    pub amount: i128,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SavingsGoalError {
    GoalNotFound = 1,
    InsufficientBalance = 2,
    GoalLocked = 3,
    Unauthorized = 4,
    TargetAmountMustBePositive = 5,
    /// Snapshot schema_version is outside the supported range
    /// (MIN_SUPPORTED_SCHEMA_VERSION..=SCHEMA_VERSION).
    UnsupportedVersion = 6,
    /// Snapshot checksum does not match the recomputed digest.
    ChecksumMismatch = 7,
}
#[contract]
pub struct SavingsGoalContract;

#[contractimpl]
impl SavingsGoalContract {
    const STORAGE_NEXT_ID: Symbol = symbol_short!("NEXT_ID");
    const STORAGE_GOALS: Symbol = symbol_short!("GOALS");
    const STORAGE_OWNER_GOAL_IDS: Symbol = symbol_short!("OWN_GOAL");

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
    fn require_not_paused(env: &Env, func: Symbol) {
        if Self::get_global_paused(env) {
            panic!("Contract is paused");
        }
        if Self::is_function_paused(env, func) {
            panic!("Function is paused");
        }
    }

    // -----------------------------------------------------------------------
    // Pause / upgrade
    // -----------------------------------------------------------------------

    /// Bootstrap storage: set NEXT_ID to 1 and GOALS to an empty map only when
    /// those keys are missing. Intended to be idempotent: calling init() more
    /// than once (e.g. from different entrypoints or upgrade paths) must not
    /// overwrite existing goals or reset NEXT_ID, to avoid ID collisions and
    /// data loss.
    pub fn init(env: Env) {
        let storage = env.storage().persistent();
        if storage.get::<_, u32>(&Self::STORAGE_NEXT_ID).is_none() {
            storage.set(&Self::STORAGE_NEXT_ID, &1u32);
        }
        if storage
            .get::<_, Map<u32, SavingsGoal>>(&Self::STORAGE_GOALS)
            .is_none()
        {
            storage.set(&Self::STORAGE_GOALS, &Map::<u32, SavingsGoal>::new(&env));
        }
    }

    pub fn set_pause_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let current = Self::get_pause_admin(&env);
        match current {
            None => {
                if caller != new_admin {
                    panic!("Unauthorized");
                }
            }
            Some(admin) if admin != caller => panic!("Unauthorized"),
            _ => {}
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSE_ADM"), &new_admin);
    }

    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).unwrap_or_else(|| panic!("No pause admin set"));
        if admin != caller {
            panic!("Unauthorized");
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &true);
        env.events()
            .publish((symbol_short!("savings"), symbol_short!("paused")), ());
    }

    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).unwrap_or_else(|| panic!("No pause admin set"));
        if admin != caller {
            panic!("Unauthorized");
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
            .publish((symbol_short!("savings"), symbol_short!("unpaused")), ());
    }

    pub fn pause_function(env: Env, caller: Address, func: Symbol) {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).unwrap_or_else(|| panic!("No pause admin set"));
        if admin != caller {
            panic!("Unauthorized");
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
    }

    pub fn unpause_function(env: Env, caller: Address, func: Symbol) {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).unwrap_or_else(|| panic!("No pause admin set"));
        if admin != caller {
            panic!("Unauthorized");
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

    /// Set or transfer the upgrade admin role.
    ///
    /// # Security Requirements
    /// - If no upgrade admin exists, caller must equal new_admin (bootstrap pattern)
    /// - If upgrade admin exists, only current upgrade admin can transfer
    /// - Caller must be authenticated via require_auth()
    ///
    /// # Parameters
    /// - `caller`: The address attempting to set the upgrade admin
    /// - `new_admin`: The address to become the new upgrade admin
    ///
    /// # Panics
    /// - If caller is unauthorized for the operation
    pub fn set_upgrade_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();

        let current_upgrade_admin = Self::get_upgrade_admin(&env);

        // Authorization logic:
        // 1. If no upgrade admin exists, caller must equal new_admin (bootstrap)
        // 2. If upgrade admin exists, only current upgrade admin can transfer
        match &current_upgrade_admin {
            None => {
                // Bootstrap pattern - caller must be setting themselves as admin
                if caller != new_admin {
                    panic!("Unauthorized: bootstrap requires caller == new_admin");
                }
            }
            Some(ref current_admin) => {
                // Admin transfer - only current admin can transfer
                if *current_admin != caller {
                    panic!("Unauthorized: only current upgrade admin can transfer");
                }
            }
        }

        env.storage()
            .instance()
            .set(&symbol_short!("UPG_ADM"), &new_admin);

        // Emit admin transfer event for audit trail
        env.events().publish(
            (symbol_short!("savings"), symbol_short!("adm_xfr")),
            (current_upgrade_admin.clone(), new_admin.clone()),
        );
    }

    /// Get the current upgrade admin address.
    ///
    /// # Returns
    /// - `Some(Address)` if upgrade admin is set
    /// - `None` if no upgrade admin has been configured
    pub fn get_upgrade_admin_public(env: Env) -> Option<Address> {
        Self::get_upgrade_admin(&env)
    }

    pub fn set_version(env: Env, caller: Address, new_version: u32) {
        caller.require_auth();
        let admin = match Self::get_upgrade_admin(&env) {
            Some(a) => a,
            None => panic!("No upgrade admin set"),
        };
        if admin != caller {
            panic!("Unauthorized");
        }
        let prev = Self::get_version(env.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("VERSION"), &new_version);
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            symbol_short!("upgraded"),
            (prev, new_version),
        );
    }

    // -----------------------------------------------------------------------
    // Tag management
    // -----------------------------------------------------------------------

    /// Validates a tag batch for metadata operations.
    ///
    /// Requirements:
    /// - At least one tag must be provided.
    /// - Each tag length must be between 1 and 32 characters.
    fn validate_tags(tags: &Vec<String>) {
        if tags.is_empty() {
            panic!("Tags cannot be empty");
        }
        for tag in tags.iter() {
            if tag.is_empty() || tag.len() > 32 {
                panic!("Tag must be between 1 and 32 characters");
            }
        }
    }

    /// Adds tags to a goal's metadata.
    ///
    /// Security:
    /// - `caller` must authorize the invocation.
    /// - Only the goal owner can add tags.
    ///
    /// Notes:
    /// - Duplicate tags are preserved as provided.
    /// - Emits `(savings, tags_add)` with `(goal_id, caller, tags)`.
    pub fn add_tags_to_goal(
        env: Env,
        caller: Address,
        goal_id: u32,
        tags: Vec<String>,
    ) {
        caller.require_auth();
        Self::validate_tags(&tags);
        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut goal = match goals.get(goal_id) {
            Some(g) => g,
            None => {
                Self::append_audit(&env, symbol_short!("add_tags"), &caller, false);
                panic!("Goal not found");
            }
        };

        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("add_tags"), &caller, false);
            panic!("Only the goal owner can add tags");
        }

        for tag in tags.iter() {
            goal.tags.push_back(tag);
        }

        goals.set(goal_id, goal);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);

        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("tags_add"),
            (goal_id, caller.clone(), tags.clone()),
        );

        Self::append_audit(&env, symbol_short!("add_tags"), &caller, true);
    }

    /// Removes tags from a goal's metadata.
    ///
    /// Security:
    /// - `caller` must authorize the invocation.
    /// - Only the goal owner can remove tags.
    ///
    /// Notes:
    /// - Removing a tag that is not present is a no-op.
    /// - Emits `(savings, tags_rem)` with `(goal_id, caller, tags)`.
    pub fn remove_tags_from_goal(
        env: Env,
        caller: Address,
        goal_id: u32,
        tags: Vec<String>,
    ) {
        caller.require_auth();
        Self::validate_tags(&tags);
        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut goal = match goals.get(goal_id) {
            Some(g) => g,
            None => {
                Self::append_audit(&env, symbol_short!("rem_tags"), &caller, false);
                panic!("Goal not found");
            }
        };

        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("rem_tags"), &caller, false);
            panic!("Only the goal owner can remove tags");
        }

        let mut new_tags = Vec::new(&env);
        for existing_tag in goal.tags.iter() {
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

        goal.tags = new_tags;
        goals.set(goal_id, goal);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);

        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("tags_rem"),
            (goal_id, caller.clone(), tags.clone()),
        );

        Self::append_audit(&env, symbol_short!("rem_tags"), &caller, true);
    }

    // -----------------------------------------------------------------------
    // Core goal operations
    // -----------------------------------------------------------------------

    /// Creates a new savings goal.
    ///
    /// - `owner` must authorize the call.
    /// - `target_amount` must be positive.
    /// - `target_date` is stored as provided and may be in the past. This
    ///   supports backfill or migration use cases where historical goals are
    ///   recorded after the fact. Callers that need strictly future-dated
    ///   goals should validate this before invoking the contract.
    ///
    /// # Events
    /// - Emits `SavingsEvent::GoalCreated`.
    pub fn create_goal(
        env: Env,
        owner: Address,
        name: String,
        target_amount: i128,
        target_date: u64,
    ) -> Result<u32, SavingsGoalsError> {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::CREATE_GOAL);

        if target_amount <= 0 {
            Self::append_audit(&env, symbol_short!("create"), &owner, false);
            return Err(SavingsGoalsError::InvalidAmount);
        }

        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32)
            + 1;

        let goal = SavingsGoal {
            id: next_id,
            owner: owner.clone(),
            name: name.clone(),
            target_amount,
            current_amount: 0,
            target_date,
            locked: true,
            unlock_date: None,
            tags: Vec::new(&env),
        };

        goals.set(next_id, goal.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);
        Self::append_owner_goal_id(&env, &owner, next_id);

        let event = GoalCreatedEvent {
            goal_id: next_id,
            name: goal.name.clone(),
            target_amount,
            target_date,
            timestamp: env.ledger().timestamp(),
        };
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            GOAL_CREATED,
            event,
        );
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("goal_new"),
            (next_id, owner),
        );

        Ok(next_id)
    }

    /// Adds funds to an existing savings goal.
    ///
    /// # Arguments
    /// * `caller` - Address of the goal owner (must authorize)
    /// * `goal_id` - ID of the goal to add funds to
    /// * `amount` - Amount to add in stroops (must be > 0)
    ///
    /// # Returns
    /// `Ok(new_total)` - The new total amount in the goal
    ///
    /// # Errors
    /// * `InvalidAmount` - If amount ≤ 0
    /// * `GoalNotFound` - If goal_id does not exist
    /// * `Unauthorized` - If caller is not the goal owner
    /// * `Overflow` - If adding amount would overflow i128
    ///
    /// # Panics
    /// * If `caller` does not authorize the transaction
    pub fn add_to_goal(
        env: Env,
        caller: Address,
        goal_id: u32,
        amount: i128,
    ) -> Result<i128, SavingsGoalsError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::ADD_TO_GOAL);

        if amount <= 0 {
            Self::append_audit(&env, symbol_short!("add"), &caller, false);
            return Err(SavingsGoalsError::InvalidAmount);
        }

        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut goal = match goals.get(goal_id) {
            Some(g) => g,
            None => {
                Self::append_audit(&env, symbol_short!("add"), &caller, false);
                return Err(SavingsGoalsError::GoalNotFound);
            }
        };

        // Access control: verify caller is the owner
        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("add"), &caller, false);
            panic!("Only the goal owner can add funds");
        }

        goal.current_amount = goal
            .current_amount
            .checked_add(amount)
            .ok_or(SavingsGoalsError::Overflow)?;
        let new_total = goal.current_amount;
        let was_completed = new_total >= goal.target_amount;
        let previously_completed = (new_total - amount) >= goal.target_amount;

        goals.set(goal_id, goal.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);

        let funds_event = FundsAddedEvent {
            goal_id,
            amount,
            new_total,
            timestamp: env.ledger().timestamp(),
        };
        RemitwiseEvents::emit(&env, EventCategory::Transaction, EventPriority::Medium, symbol_short!("funds_add"), funds_event);

        if was_completed && !previously_completed {
            let completed_event = GoalCompletedEvent {
                goal_id,
                name: goal.name.clone(),
                final_amount: new_total,
                timestamp: env.ledger().timestamp(),
            };
            env.events().publish((GOAL_COMPLETED,), completed_event);
        }

        Self::append_audit(&env, symbol_short!("add"), &caller, true);
        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::FundsAdded),
            (goal_id, caller.clone(), amount),
        );

        if was_completed && !previously_completed {
            env.events().publish(
                (symbol_short!("savings"), SavingsEvent::GoalCompleted),
                (goal_id, caller),
            );
        }

        Ok(new_total)
    }

    pub fn batch_add_to_goals(
        env: Env,
        caller: Address,
        contributions: Vec<ContributionItem>,
    ) -> Result<u32, SavingsGoalsError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::ADD_TO_GOAL);
        if contributions.len() > MAX_BATCH_SIZE {
            return Err(SavingsGoalsError::InvalidAmount);
        }
        let goals_map: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));
        for item in contributions.iter() {
            if item.amount <= 0 {
                return Err(SavingsGoalsError::InvalidAmount);
            }
            let goal = match goals_map.get(item.goal_id) {
                Some(g) => g,
                None => return Err(SavingsGoalsError::GoalNotFound),
            };
            if goal.owner != caller {
                return Err(SavingsGoalsError::Unauthorized);
            }
        }
        Self::extend_instance_ttl(&env);
        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));
        let mut count = 0u32;
        for item in contributions.iter() {
            let mut goal = match goals.get(item.goal_id) {
                Some(g) => g,
                None => return Err(SavingsGoalsError::GoalNotFound),
            };
            if goal.owner != caller {
                return Err(SavingsGoalsError::Unauthorized);
            }
            goal.current_amount = match goal.current_amount.checked_add(item.amount) {
                Some(v) => v,
                None => panic!("overflow"),
            };
            let new_total = goal.current_amount;
            let was_completed = new_total >= goal.target_amount;
            let previously_completed = (new_total - item.amount) >= goal.target_amount;
            goals.set(item.goal_id, goal.clone());
            let funds_event = FundsAddedEvent {
                goal_id: item.goal_id,
                amount: item.amount,
                new_total,
                timestamp: env.ledger().timestamp(),
            };
            RemitwiseEvents::emit(
                &env,
                EventCategory::Transaction,
                EventPriority::Medium,
                symbol_short!("funds_add"),
                funds_event,
            );
            if was_completed && !previously_completed {
                let completed_event = GoalCompletedEvent {
                    goal_id: item.goal_id,
                    name: goal.name.clone(),
                    final_amount: new_total,
                    timestamp: env.ledger().timestamp(),
                };
                env.events().publish((GOAL_COMPLETED,), completed_event);
            }
            env.events().publish(
                (symbol_short!("savings"), SavingsEvent::FundsAdded),
                (item.goal_id, caller.clone(), item.amount),
            );
            if was_completed && !previously_completed {
                env.events().publish(
                    (symbol_short!("savings"), SavingsEvent::GoalCompleted),
                    (item.goal_id, caller.clone()),
                );
            }
            count += 1;
        }
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);
        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::Medium,
            symbol_short!("batch_add"),
            (count, caller),
        );
        Ok(count)
    }

    /// Withdraws funds from an existing savings goal.
    ///
    /// # Arguments
    /// * `caller` - Address of the goal owner (must authorize)
    /// * `goal_id` - ID of the goal to withdraw from
    /// * `amount` - Amount to withdraw in stroops (must be > 0)
    ///
    /// # Returns
    /// `Ok(remaining_amount)` - The remaining amount in the goal after withdrawal
    ///
    /// # Errors
    /// * `InvalidAmount` - If amount ≤ 0
    /// * `GoalNotFound` - If goal_id does not exist
    /// * `Unauthorized` - If caller is not the goal owner
    /// * `GoalLocked` - If goal is locked or time-locked
    /// * `InsufficientBalance` - If amount > current_amount
    /// * `Overflow` - If subtraction would underflow i128
    ///
    /// # Panics
    /// * If `caller` does not authorize the transaction
    /// Withdraws funds from an existing savings goal.
    ///
    /// # Arguments
    /// * `caller` - Address of the goal owner (must authorize)
    /// * `goal_id` - ID of the goal to withdraw from
    /// * `amount` - Amount to withdraw in stroops (must be > 0)
    ///
    /// # Returns
    /// `Ok(remaining_amount)` - The remaining amount in the goal after withdrawal
    ///
    /// # Errors
    /// * `InvalidAmount` - If amount ≤ 0
    /// * `GoalNotFound` - If goal_id does not exist
    /// * `Unauthorized` - If caller is not the goal owner
    /// * `InsufficientBalance` - If amount > current_amount
    /// * `GoalLocked` - If the goal is locked or time-lock has not expired
    ///
    /// # Time-lock Behavior
    /// - If `unlock_date` is set, withdrawal will fail if `env.ledger().timestamp() < unlock_date`.
    /// - Boundary condition: Success if `timestamp == unlock_date`.
    ///
    /// # Events
    /// - Emits `SavingsEvent::FundsWithdrawn`.
    pub fn withdraw_from_goal(
        env: Env,
        caller: Address,
        goal_id: u32,
        amount: i128,
    ) -> Result<i128, SavingsGoalsError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::WITHDRAW);

        if amount <= 0 {
            Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
            return Err(SavingsGoalsError::InvalidAmount);
        }

        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut goal = match goals.get(goal_id) {
            Some(g) => g,
            None => {
                Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
                return Err(SavingsGoalsError::GoalNotFound);
            }
        };

        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
            return Err(SavingsGoalsError::Unauthorized);
        }

        if goal.locked {
            Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
            return Err(SavingsGoalsError::GoalLocked);
        }

        if let Some(unlock_date) = goal.unlock_date {
            let current_time = env.ledger().timestamp();
            if current_time < unlock_date {
                Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
                return Err(SavingsGoalsError::GoalLocked);
            }
        }

        if amount > goal.current_amount {
            Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
            return Err(SavingsGoalsError::InsufficientBalance);
        }

        goal.current_amount = goal
            .current_amount
            .checked_sub(amount)
            .ok_or(SavingsGoalsError::Overflow)?;
        let new_amount = goal.current_amount;

        goals.set(goal_id, goal);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);

        Self::append_audit(&env, symbol_short!("withdraw"), &caller, true);
        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::FundsWithdrawn),
            (goal_id, caller, amount),
        );

        Ok(new_amount)
    }

    /// Locks a goal to prevent manual withdrawals.
    ///
    /// # Arguments
    /// * `caller` - Address of the goal owner
    /// * `goal_id` - ID of the goal
    ///
    /// # Events
    /// - Emits `SavingsEvent::GoalLocked`.
    pub fn lock_goal(env: Env, caller: Address, goal_id: u32) -> bool {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::LOCK);
        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut goal = match goals.get(goal_id) {
            Some(g) => g,
            None => {
                Self::append_audit(&env, symbol_short!("lock"), &caller, false);
                panic!("Goal not found");
            }
        };

        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("lock"), &caller, false);
            panic!("Only the goal owner can lock this goal");
        }

        if goal.locked {
            return true;
        }

        goal.locked = true;
        goals.set(goal_id, goal);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);

        Self::append_audit(&env, symbol_short!("lock"), &caller, true);
        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::GoalLocked),
            (goal_id, caller),
        );

        true
    }

    /// Unlocks a goal for manual withdrawals.
    ///
    /// # Arguments
    /// * `caller` - Address of the goal owner
    /// * `goal_id` - ID of the goal
    ///
    /// # Events
    /// - Emits `SavingsEvent::GoalUnlocked`.
    pub fn unlock_goal(env: Env, caller: Address, goal_id: u32) -> bool {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::UNLOCK);
        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut goal = match goals.get(goal_id) {
            Some(g) => g,
            None => {
                Self::append_audit(&env, symbol_short!("unlock"), &caller, false);
                panic!("Goal not found");
            }
        };

        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("unlock"), &caller, false);
            panic!("Only the goal owner can unlock this goal");
        }

        if !goal.locked {
            return true;
        }

        goal.locked = false;
        goals.set(goal_id, goal);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);

        Self::append_audit(&env, symbol_short!("unlock"), &caller, true);
        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::GoalUnlocked),
            (goal_id, caller),
        );

        true
    }

    pub fn get_goal(env: Env, goal_id: u32) -> Option<SavingsGoal> {
        let goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));
        goals.get(goal_id)
    }

    // -----------------------------------------------------------------------
    // PAGINATED LIST QUERIES
    // -----------------------------------------------------------------------

    /// @notice Returns a deterministic page of goals for one owner.
    /// @dev Paging order is anchored to the owner-goal ID index (append-only,
    ///      ascending by creation ID), not map iteration order.
    /// @dev `cursor` is exclusive and must match an existing goal ID in the
    ///      owner's index when non-zero; invalid cursors are rejected.
    ///
    /// # Arguments
    /// * `owner`  - whose goals to return
    /// * `cursor` - start after this goal ID (pass 0 for the first page)
    /// * `limit`  - max items per page (0 -> DEFAULT_PAGE_LIMIT, capped at MAX_PAGE_LIMIT)
    ///
    /// # Returns
    /// `GoalPage { items, next_cursor, count }`.
    /// `next_cursor == 0` means no more pages.
    pub fn get_goals(env: Env, owner: Address, cursor: u32, limit: u32) -> GoalPage {
        let limit = Self::clamp_limit(limit);
        let goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let owner_goal_ids: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&Self::STORAGE_OWNER_GOAL_IDS)
            .unwrap_or_else(|| Map::new(&env));
        let ids = owner_goal_ids
            .get(owner.clone())
            .unwrap_or_else(|| Vec::new(&env));

        if ids.is_empty() {
            return GoalPage {
                items: Vec::new(&env),
                next_cursor: 0,
                count: 0,
            };
        }

        let mut start_index: u32 = 0;
        if cursor != 0 {
            let mut found = false;
            for i in 0..ids.len() {
                if ids.get(i) == Some(cursor) {
                    start_index = i + 1;
                    found = true;
                    break;
                }
            }
            if !found {
                panic!("Invalid cursor");
            }
        }

        let mut end_index = start_index + limit;
        if end_index > ids.len() {
            end_index = ids.len();
        }

        let mut result = Vec::new(&env);
        for i in start_index..end_index {
            let goal_id = ids
                .get(i)
                .unwrap_or_else(|| panic!("Pagination index out of sync"));
            let goal = goals
                .get(goal_id)
                .unwrap_or_else(|| panic!("Pagination index out of sync"));
            if goal.owner != owner {
                panic!("Pagination index owner mismatch");
            }
            result.push_back(goal);
        }

        let next_cursor = if end_index < ids.len() {
            ids.get(end_index - 1)
                .unwrap_or_else(|| panic!("Pagination index out of sync"))
        } else {
            0
        };

        GoalPage {
            items: result,
            next_cursor,
            count: end_index - start_index,
        }
    }

    /// Backward-compatible: returns ALL goals for owner in one Vec.
    /// Prefer the paginated `get_goals` for production use.
    pub fn get_all_goals(env: Env, owner: Address) -> Vec<SavingsGoal> {
        let goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));
        let mut result = Vec::new(&env);
        for (_, goal) in goals.iter() {
            if goal.owner == owner {
                result.push_back(goal);
            }
        }
        result
    }

    pub fn is_goal_completed(env: Env, goal_id: u32) -> bool {
        let storage = env.storage().instance();
        let goals: Map<u32, SavingsGoal> = storage
            .get(&symbol_short!("GOALS"))
            .unwrap_or(Map::new(&env));
        if let Some(goal) = goals.get(goal_id) {
            goal.current_amount >= goal.target_amount
        } else {
            false
        }
    }

    // -----------------------------------------------------------------------
    // Snapshot, audit, schedule
    // -----------------------------------------------------------------------

    pub fn get_nonce(env: Env, address: Address) -> u64 {
        let nonces: Option<Map<Address, u64>> =
            env.storage().instance().get(&symbol_short!("NONCES"));
        nonces
            .as_ref()
            .and_then(|m: &Map<Address, u64>| m.get(address))
            .unwrap_or(0)
    }

    pub fn export_snapshot(env: Env, caller: Address) -> GoalsExportSnapshot {
        caller.require_auth();
        let goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));
        let next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32);
        let mut list = Vec::new(&env);
        for i in 1..=next_id {
            if let Some(g) = goals.get(i) {
                list.push_back(g);
            }
        }
        let checksum = Self::compute_goals_checksum(SCHEMA_VERSION, next_id, &list);
        env.events().publish(
            (symbol_short!("goals"), symbol_short!("snap_exp")),
            SCHEMA_VERSION,
        );
        GoalsExportSnapshot {
            schema_version: SCHEMA_VERSION,
            checksum,
            next_id,
            goals: list,
        }
    }

    pub fn import_snapshot(
        env: Env,
        caller: Address,
        nonce: u64,
        snapshot: GoalsExportSnapshot,
    ) -> Result<bool, SavingsGoalError> {
        caller.require_auth();
        Self::require_nonce(&env, &caller, nonce);

        // Accept any schema_version within the supported range for backward/forward compat.
        if snapshot.schema_version < MIN_SUPPORTED_SCHEMA_VERSION
            || snapshot.schema_version > SCHEMA_VERSION
        {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(SavingsGoalError::UnsupportedVersion);
        }
        let expected = Self::compute_goals_checksum(
            snapshot.schema_version,
            snapshot.next_id,
            &snapshot.goals,
        );
        if snapshot.checksum != expected {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(SavingsGoalError::ChecksumMismatch);
        }

        Self::extend_instance_ttl(&env);
        let mut goals: Map<u32, SavingsGoal> = Map::new(&env);
        let mut owner_goal_ids: Map<Address, Vec<u32>> = Map::new(&env);
        for g in snapshot.goals.iter() {
            goals.set(g.id, g.clone());
            let mut ids = owner_goal_ids
                .get(g.owner.clone())
                .unwrap_or_else(|| Vec::new(&env));
            ids.push_back(g.id);
            owner_goal_ids.set(g.owner.clone(), ids);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &snapshot.next_id);
        env.storage()
            .instance()
            .set(&Self::STORAGE_OWNER_GOAL_IDS, &owner_goal_ids);

        Self::increment_nonce(&env, &caller);
        Self::append_audit(&env, symbol_short!("import"), &caller, true);
        Ok(true)
    }

    pub fn get_audit_log(env: Env, from_index: u32, limit: u32) -> Vec<AuditEntry> {
        let log: Option<Vec<AuditEntry>> = env.storage().instance().get(&symbol_short!("AUDIT"));
        let log = log.unwrap_or_else(|| Vec::new(&env));
        let len = log.len();
        let cap = MAX_AUDIT_ENTRIES.min(limit);
        let mut out = Vec::new(&env);
        if from_index >= len {
            return out;
        }
        let end = (from_index + cap).min(len);
        for i in from_index..end {
            if let Some(entry) = log.get(i) {
                out.push_back(entry);
            }
        }
        out
    }

    fn require_nonce(env: &Env, address: &Address, expected: u64) {
        let current = Self::get_nonce(env.clone(), address.clone());
        if expected != current {
            panic!("Invalid nonce: expected {}, got {}", current, expected);
        }
    }

    fn increment_nonce(env: &Env, address: &Address) {
        let current = Self::get_nonce(env.clone(), address.clone());
        let next = match current.checked_add(1) {
            Some(v) => v,
            None => panic!("nonce overflow"),
        };
        let mut nonces: Map<Address, u64> = env
            .storage()
            .instance()
            .get(&symbol_short!("NONCES"))
            .unwrap_or_else(|| Map::new(env));
        nonces.set(address.clone(), next);
        env.storage()
            .instance()
            .set(&symbol_short!("NONCES"), &nonces);
    }

    fn compute_goals_checksum(version: u32, next_id: u32, goals: &Vec<SavingsGoal>) -> u64 {
        let mut c = version as u64 + next_id as u64;
        for i in 0..goals.len() {
            if let Some(g) = goals.get(i) {
                c = c
                    .wrapping_add(g.id as u64)
                    .wrapping_add(g.target_amount as u64)
                    .wrapping_add(g.current_amount as u64);
            }
        }
        c.wrapping_mul(31)
    }

    fn append_audit(env: &Env, operation: Symbol, caller: &Address, success: bool) {
        let timestamp = env.ledger().timestamp();
        let mut log: Vec<AuditEntry> = env
            .storage()
            .instance()
            .get(&symbol_short!("AUDIT"))
            .unwrap_or_else(|| Vec::new(env));
        if log.len() >= MAX_AUDIT_ENTRIES {
            let mut new_log = Vec::new(env);
            for i in 1..log.len() {
                if let Some(entry) = log.get(i) {
                    new_log.push_back(entry);
                }
            }
            log = new_log;
        }
        log.push_back(AuditEntry {
            operation,
            caller: caller.clone(),
            timestamp,
            success,
        });
        env.storage().instance().set(&symbol_short!("AUDIT"), &log);
    }

    #[allow(dead_code)]
    fn get_owner_goal_ids_map(env: &Env) -> Option<Map<Address, Vec<u32>>> {
        env.storage().instance().get(&Self::STORAGE_OWNER_GOAL_IDS)
    }

    fn append_owner_goal_id(env: &Env, owner: &Address, goal_id: u32) {
        let mut owner_goal_ids: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&Self::STORAGE_OWNER_GOAL_IDS)
            .unwrap_or_else(|| Map::new(env));
        let mut ids = owner_goal_ids
            .get(owner.clone())
            .unwrap_or_else(|| Vec::new(env));
        ids.push_back(goal_id);
        owner_goal_ids.set(owner.clone(), ids);
        env.storage()
            .instance()
            .set(&Self::STORAGE_OWNER_GOAL_IDS, &owner_goal_ids);
    }

    /// Extend the TTL of instance storage
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Set time-lock on a goal
    /// Sets a time-lock on a savings goal.
    ///
    /// # Arguments
    /// * `caller` - Address of the goal owner
    /// * `goal_id` - ID of the goal
    /// * `unlock_date` - Unix timestamp when the goal becomes withdrawable
    ///
    /// # Panics
    /// - If caller is not the owner or goal not found.
    pub fn set_time_lock(env: Env, caller: Address, goal_id: u32, unlock_date: u64) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut goal = match goals.get(goal_id) {
            Some(g) => g,
            None => {
                Self::append_audit(&env, symbol_short!("timelock"), &caller, false);
                panic!("Goal not found");
            }
        };

        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("timelock"), &caller, false);
            panic!("Only the goal owner can set time-lock");
        }

        let current_time = env.ledger().timestamp();
        if unlock_date <= current_time {
            Self::append_audit(&env, symbol_short!("timelock"), &caller, false);
            panic!("Unlock date must be in the future");
        }

        goal.unlock_date = Some(unlock_date);
        goals.set(goal_id, goal);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);

        Self::append_audit(&env, symbol_short!("timelock"), &caller, true);
        true
    }

    /// Creates a recurring savings schedule.
    ///
    /// # Arguments
    /// * `owner` - Address of the schedule owner
    /// * `goal_id` - ID of the goal to fund
    /// * `amount` - Amount to save in each interval
    /// * `next_due` - First execution timestamp
    /// * `interval` - Seconds between executions
    ///
    /// # Returns
    /// - ID of the new schedule
    pub fn create_savings_schedule(
        env: Env,
        owner: Address,
        goal_id: u32,
        amount: i128,
        next_due: u64,
        interval: u64,
    ) -> u32 {
        owner.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let goal = match goals.get(goal_id) {
            Some(g) => g,
            None => panic!("Goal not found"),
        };

        if goal.owner != owner {
            panic!("Only the goal owner can create schedules");
        }

        let current_time = env.ledger().timestamp();
        if next_due <= current_time {
            panic!("Next due date must be in the future");
        }

        Self::extend_instance_ttl(&env);

        let mut schedules: Map<u32, SavingsSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("SAV_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        let next_schedule_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_SSCH"))
            .unwrap_or(0u32)
            + 1;

        let schedule = SavingsSchedule {
            id: next_schedule_id,
            owner: owner.clone(),
            goal_id,
            amount,
            next_due,
            interval,
            recurring: interval > 0,
            active: true,
            created_at: current_time,
            last_executed: None,
            missed_count: 0,
        };

        schedules.set(next_schedule_id, schedule);
        env.storage()
            .instance()
            .set(&symbol_short!("SAV_SCH"), &schedules);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_SSCH"), &next_schedule_id);

        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::ScheduleCreated),
            (next_schedule_id, owner),
        );

        next_schedule_id
    }

    pub fn modify_savings_schedule(
        env: Env,
        caller: Address,
        schedule_id: u32,
        amount: i128,
        next_due: u64,
        interval: u64,
    ) -> bool {
        caller.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let current_time = env.ledger().timestamp();
        if next_due <= current_time {
            panic!("Next due date must be in the future");
        }

        Self::extend_instance_ttl(&env);

        let mut schedules: Map<u32, SavingsSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("SAV_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        let mut schedule = match schedules.get(schedule_id) {
            Some(s) => s,
            None => panic!("Schedule not found"),
        };

        if schedule.owner != caller {
            panic!("Only the schedule owner can modify it");
        }

        schedule.amount = amount;
        schedule.next_due = next_due;
        schedule.interval = interval;
        schedule.recurring = interval > 0;

        schedules.set(schedule_id, schedule);
        env.storage()
            .instance()
            .set(&symbol_short!("SAV_SCH"), &schedules);

        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::ScheduleModified),
            (schedule_id, caller),
        );

        true
    }

    pub fn cancel_savings_schedule(env: Env, caller: Address, schedule_id: u32) -> bool {
        caller.require_auth();

        Self::extend_instance_ttl(&env);

        let mut schedules: Map<u32, SavingsSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("SAV_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        let mut schedule = match schedules.get(schedule_id) {
            Some(s) => s,
            None => panic!("Schedule not found"),
        };

        if schedule.owner != caller {
            panic!("Only the schedule owner can cancel it");
        }

        schedule.active = false;

        schedules.set(schedule_id, schedule);
        env.storage()
            .instance()
            .set(&symbol_short!("SAV_SCH"), &schedules);

        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::ScheduleCancelled),
            (schedule_id, caller),
        );

        true
    }

    /// Executes all savings schedules whose `next_due` timestamp is at or before
    /// the current ledger timestamp.
    ///
    /// # Idempotency guarantee
    /// A schedule is skipped if its `last_executed` timestamp is greater than or
    /// equal to its `next_due` timestamp at the time of the call.  This prevents
    /// double-crediting a goal when `execute_due_savings_schedules` is invoked
    /// multiple times within the same execution window – for example, two
    /// transactions landing in the same Stellar ledger (which share a ledger
    /// timestamp), or a retry after a transient failure.
    ///
    /// # Next-due advancement
    /// * **Recurring schedules** (`interval > 0`): `next_due` is advanced by
    ///   `interval` until it is strictly greater than `current_time`.  Any
    ///   skipped intervals increment `missed_count`.
    /// * **One-shot schedules** (`interval == 0`): the schedule is deactivated
    ///   (`active = false`) after a single execution.
    ///
    /// # Returns
    /// A vector of schedule IDs that were executed in this call.
    ///
    /// # Drift Handling
    /// - If execution is delayed, the schedule will "catch up" by skipping missed intervals
    ///   and incrementing `missed_count`.
    /// - `next_due` is set to the next future interval anchor.
    ///
    /// # Events
    /// - Emits `SavingsEvent::ScheduleExecuted` for each successful execution.
    /// - Emits `SavingsEvent::ScheduleMissed` for each interval missed.
    ///
    /// # Security assumptions
    /// * `last_executed` is written by this function only **after** a
    ///   successful credit to the goal.  It is never reset by other functions,
    ///   so an attacker cannot clear it to trigger re-execution.
    /// * `modify_savings_schedule` resets `next_due` to a future timestamp
    ///   supplied by the owner.  A new `next_due > last_executed` correctly
    ///   re-enables execution for the updated due date.
    pub fn execute_due_savings_schedules(env: Env) -> Vec<u32> {
        Self::extend_instance_ttl(&env);

        let current_time = env.ledger().timestamp();
        let mut executed = Vec::new(&env);

        let mut schedules: Map<u32, SavingsSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("SAV_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        for (schedule_id, mut schedule) in schedules.iter() {
            if !schedule.active || schedule.next_due > current_time {
                continue;
            }

            // Idempotency guard: skip if this schedule was already executed at
            // or after its current `next_due`.  Prevents double-crediting when
            // this function is called more than once within the same execution
            // window (same ledger timestamp) or retried after a partial run.
            if let Some(last_exec) = schedule.last_executed {
                if last_exec >= schedule.next_due {
                    continue;
                }
            }

            if let Some(mut goal) = goals.get(schedule.goal_id) {
                goal.current_amount = match goal.current_amount.checked_add(schedule.amount) {
                    Some(v) => v,
                    None => panic!("overflow"),
                };

                let is_completed = goal.current_amount >= goal.target_amount;
                goals.set(schedule.goal_id, goal.clone());

                env.events().publish(
                    (symbol_short!("savings"), SavingsEvent::FundsAdded),
                    (schedule.goal_id, goal.owner.clone(), schedule.amount),
                );

                if is_completed {
                    env.events().publish(
                        (symbol_short!("savings"), SavingsEvent::GoalCompleted),
                        (schedule.goal_id, goal.owner),
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
                        (symbol_short!("savings"), SavingsEvent::ScheduleMissed),
                        (schedule_id, missed),
                    );
                }
            } else {
                schedule.active = false;
            }

            schedules.set(schedule_id, schedule);
            executed.push_back(schedule_id);

            env.events().publish(
                (symbol_short!("savings"), SavingsEvent::ScheduleExecuted),
                schedule_id,
            );
        }

        env.storage()
            .instance()
            .set(&symbol_short!("SAV_SCH"), &schedules);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);

        executed
    }

    pub fn get_savings_schedules(env: Env, owner: Address) -> Vec<SavingsSchedule> {
        let schedules: Map<u32, SavingsSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("SAV_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        for (_, schedule) in schedules.iter() {
            if schedule.owner == owner {
                result.push_back(schedule);
            }
        }
        result
    }

    pub fn get_savings_schedule(env: Env, schedule_id: u32) -> Option<SavingsSchedule> {
        let schedules: Map<u32, SavingsSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("SAV_SCH"))
            .unwrap_or_else(|| Map::new(&env));
        schedules.get(schedule_id)
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------
#[cfg(test)]
mod test;
