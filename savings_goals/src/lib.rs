#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
use remitwise_common::{
    EventCategory, EventPriority, RemitwiseEvents, CONTRACT_VERSION, DEFAULT_PAGE_LIMIT,
    INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD, MAX_BATCH_SIZE, MAX_PAGE_LIMIT,
    PERSISTENT_BUMP_AMOUNT, PERSISTENT_LIFETIME_THRESHOLD,
};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, String,
    Symbol, Vec,
};
use remitwise_common::{EventCategory, EventPriority, RemitwiseEvents};

// Event topics
const GOAL_CREATED: Symbol = symbol_short!("created");
const FUNDS_ADDED: Symbol = symbol_short!("added");
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

#[contracttype]
#[derive(Clone)]
pub struct GoalPage {
    pub items: Vec<SavingsGoal>,
    pub next_cursor: u32,
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

<<<<<<< HEAD
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SavingsGoalError {
=======
#[contracttype]
#[derive(Clone, Copy, Debug)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavingsGoalsError {
>>>>>>> origin/main
    InvalidAmount = 1,
    GoalNotFound = 2,
    Unauthorized = 3,
    GoalLocked = 4,
    InsufficientBalance = 5,
    Overflow = 6,
    TargetAmountMustBePositive = 7,
    UnsupportedVersion = 8,
    ChecksumMismatch = 9,
}

#[contracttype]
#[derive(Clone)]
pub struct GoalsExportSnapshot {
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

const SCHEMA_VERSION: u32 = 1;
const MIN_SUPPORTED_SCHEMA_VERSION: u32 = 1;
const MAX_AUDIT_ENTRIES: u32 = 100;

pub mod pause_functions {
    use soroban_sdk::{symbol_short, Symbol};
    pub const CREATE_GOAL: Symbol = symbol_short!("crt_goal");
    pub const ADD_TO_GOAL: Symbol = symbol_short!("add_goal");
    pub const WITHDRAW: Symbol = symbol_short!("withdraw");
    pub const LOCK: Symbol = symbol_short!("lock");
    pub const UNLOCK: Symbol = symbol_short!("unlock");
    pub const SET_TIME_LOCK: Symbol = symbol_short!("set_tlk");
}

#[contracttype]
#[derive(Clone)]
pub struct ContributionItem {
    pub goal_id: u32,
    pub amount: i128,
}

#[contract]
pub struct SavingsGoalContract;

#[contractimpl]
impl SavingsGoalContract {
    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn clamp_limit(limit: u32) -> u32 {
        if limit == 0 {
            DEFAULT_PAGE_LIMIT
        } else {
            limit.min(MAX_PAGE_LIMIT)
        }
    }

    fn require_not_paused(env: &Env, func: Symbol) {
        if env
            .storage()
            .instance()
            .get::<_, bool>(&symbol_short!("PAUSED"))
            .unwrap_or(false)
        {
            panic!("Contract is paused");
        }
        let m: Map<Symbol, bool> = env
            .storage()
            .instance()
            .get(&symbol_short!("PAUSED_FN"))
            .unwrap_or_else(|| Map::new(env));
        if m.get(func).unwrap_or(false) {
            panic!("Function is paused");
        }
    }

    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn extend_persistent_ttl(env: &Env, key: &impl soroban_sdk::IntoVal<Env, soroban_sdk::Val>) {
        env.storage().persistent().extend_ttl(
            key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    // -----------------------------------------------------------------------
    // Contract Lifecycle
    // -----------------------------------------------------------------------

    pub fn init(env: Env) {
        let s = env.storage().instance();
        if !s.has(&symbol_short!("NEXT_ID")) {
            s.set(&symbol_short!("NEXT_ID"), &0u32);
        }
        if !s.has(&symbol_short!("NEXT_SCH")) {
            s.set(&symbol_short!("NEXT_SCH"), &0u32);
        }
        Self::extend_instance_ttl(&env);
    }

    pub fn set_pause_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let s = env.storage().instance();
        let current: Option<Address> = s.get(&symbol_short!("PAUSE_ADM"));
        if let Some(admin) = current {
            if admin != caller {
                panic!("Unauthorized");
            }
        } else if caller != new_admin {
            panic!("Unauthorized");
        }
        s.set(&symbol_short!("PAUSE_ADM"), &new_admin);
        Self::extend_instance_ttl(&env);
    }

    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("PAUSE_ADM"))
            .unwrap_or_else(|| panic!("No admin"));
        if admin != caller {
            panic!("Unauthorized");
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &true);
        Self::extend_instance_ttl(&env);
    }

    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("PAUSE_ADM"))
            .unwrap_or_else(|| panic!("No admin"));
        if admin != caller {
            panic!("Unauthorized");
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &false);
        Self::extend_instance_ttl(&env);
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("VERSION"))
            .unwrap_or(CONTRACT_VERSION)
    }

<<<<<<< HEAD
=======
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
        } else if caller != new_admin {
            panic!("Unauthorized: bootstrap requires caller == new_admin");
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

>>>>>>> origin/main
    // -----------------------------------------------------------------------
    // Core Logic (Scalable Storage)
    // -----------------------------------------------------------------------

<<<<<<< HEAD
=======
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
            if tag.len() == 0 || tag.len() > 32 {
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

        let mut goal = goals.get(goal_id).expect("Goal not found");

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

        let mut goal = goals.get(goal_id).expect("Goal not found");

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
    /// - Emits `GOAL_CREATED` with goal details.
    /// - Emits `SavingsEvent::GoalCreated`.
>>>>>>> origin/main
    pub fn create_goal(
        env: Env,
        owner: Address,
        name: String,
        target_amount: i128,
        target_date: u64,
    ) -> Result<u32, SavingsGoalError> {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::CREATE_GOAL);
        if target_amount <= 0 {
            Self::append_audit(&env, symbol_short!("create"), &owner, false);
            return Err(SavingsGoalError::InvalidAmount);
        }

        Self::extend_instance_ttl(&env);
        let mut next_id: u32 = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0);
        next_id += 1;
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);

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
        Self::set_goal_data(&env, next_id, &goal);
        Self::append_to_owner_goal_ids(&env, &owner, next_id);

<<<<<<< HEAD
=======
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
>>>>>>> origin/main
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
<<<<<<< HEAD
            GOAL_CREATED,
            GoalCreatedEvent {
                goal_id: next_id,
                name,
                target_amount,
                target_date,
                timestamp: env.ledger().timestamp(),
            },
=======
            symbol_short!("created"),
            event,
        );
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("goal_new"),
            (next_id, owner),
>>>>>>> origin/main
        );
        Self::append_audit(&env, symbol_short!("create"), &owner, true);
        Ok(next_id)
    }

    pub fn add_to_goal(
        env: Env,
        caller: Address,
        goal_id: u32,
        amount: i128,
    ) -> Result<i128, SavingsGoalError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::ADD_TO_GOAL);
        Self::extend_instance_ttl(&env);
        if amount <= 0 {
            Self::append_audit(&env, symbol_short!("add"), &caller, false);
            return Err(SavingsGoalError::InvalidAmount);
        }

        let mut goal = Self::get_goal_data(&env, goal_id).ok_or(SavingsGoalError::GoalNotFound)?;
        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("add"), &caller, false);
            panic!("Not owner");
        }

        goal.current_amount = goal
            .current_amount
            .checked_add(amount)
            .ok_or(SavingsGoalError::Overflow)?;
        let was_completed = goal.current_amount >= goal.target_amount;
        let previously_completed = (goal.current_amount - amount) >= goal.target_amount;

        Self::set_goal_data(&env, goal_id, &goal);

<<<<<<< HEAD
        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::Medium,
            FUNDS_ADDED,
            FundsAddedEvent {
=======
        let funds_event = FundsAddedEvent {
            goal_id,
            amount,
            new_total,
            timestamp: env.ledger().timestamp(),
        };
        RemitwiseEvents::emit(&env, EventCategory::Transaction, EventPriority::Medium, symbol_short!("funds_add"), funds_event);

        if was_completed && !previously_completed {
            let completed_event = GoalCompletedEvent {
>>>>>>> origin/main
                goal_id,
                amount,
                new_total: goal.current_amount,
                timestamp: env.ledger().timestamp(),
            },
        );
        if was_completed && !previously_completed {
            RemitwiseEvents::emit(
                &env,
                EventCategory::Transaction,
                EventPriority::High,
                GOAL_COMPLETED,
                GoalCompletedEvent {
                    goal_id,
                    name: goal.name,
                    final_amount: goal.current_amount,
                    timestamp: env.ledger().timestamp(),
                },
            );
        }
        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::Medium,
            symbol_short!("funds_add"),
            (goal_id, caller.clone(), amount),
        );
        Self::append_audit(&env, symbol_short!("add"), &caller, true);
        Ok(goal.current_amount)
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

        Self::extend_instance_ttl(&env);
        let mut count = 0u32;
        for item in contributions.iter() {
            if item.amount <= 0 {
                return Err(SavingsGoalsError::InvalidAmount);
            }
<<<<<<< HEAD
            let mut goal =
                Self::get_goal_data(&env, item.goal_id).unwrap_or_else(|| panic!("Goal not found"));
            if goal.owner != caller {
                panic!("Not owner");
            }

            goal.current_amount = goal
                .current_amount
                .checked_add(item.amount)
                .unwrap_or_else(|| panic!("Overflow"));
            let key = (symbol_short!("GOAL_D"), item.goal_id);
            env.storage().persistent().set(&key, &goal);
            count += 1;
        }
        Self::append_audit(&env, symbol_short!("batch_add"), &caller, true);
        count
=======
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
>>>>>>> origin/main
    }

    pub fn withdraw_from_goal(
        env: Env,
        caller: Address,
        goal_id: u32,
        amount: i128,
    ) -> Result<i128, SavingsGoalError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::WITHDRAW);
        Self::extend_instance_ttl(&env);
        if amount <= 0 {
            return Err(SavingsGoalError::InvalidAmount);
        }

        let mut goal = Self::get_goal_data(&env, goal_id).ok_or(SavingsGoalError::GoalNotFound)?;
        if goal.owner != caller {
            return Err(SavingsGoalError::Unauthorized);
        }
        if goal.locked {
            return Err(SavingsGoalError::GoalLocked);
        }
        if let Some(unlock) = goal.unlock_date {
            if env.ledger().timestamp() < unlock {
                return Err(SavingsGoalError::GoalLocked);
            }
        }
        if amount > goal.current_amount {
            return Err(SavingsGoalError::InsufficientBalance);
        }

        goal.current_amount = goal
            .current_amount
            .checked_sub(amount)
            .ok_or(SavingsGoalError::Overflow)?;
        Self::set_goal_data(&env, goal_id, &goal);

        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::Medium,
            symbol_short!("funds_wit"),
            (goal_id, caller.clone(), amount),
        );
        Self::append_audit(&env, symbol_short!("withdraw"), &caller, true);
        Ok(goal.current_amount)
    }

    pub fn lock_goal(env: Env, owner: Address, goal_id: u32) {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::LOCK);
        Self::extend_instance_ttl(&env);
        let mut goal =
            Self::get_goal_data(&env, goal_id).unwrap_or_else(|| panic!("Goal not found"));
        if goal.owner != owner {
            panic!("Unauthorized");
        }
        goal.locked = true;
        Self::set_goal_data(&env, goal_id, &goal);
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Low,
            symbol_short!("locked"),
            (goal_id, owner),
        );
    }

    pub fn unlock_goal(env: Env, owner: Address, goal_id: u32) {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::UNLOCK);
        Self::extend_instance_ttl(&env);
        let mut goal =
            Self::get_goal_data(&env, goal_id).unwrap_or_else(|| panic!("Goal not found"));
        if goal.owner != owner {
            panic!("Unauthorized");
        }
        goal.locked = false;
        Self::set_goal_data(&env, goal_id, &goal);
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Low,
            symbol_short!("unlocked"),
            (goal_id, owner),
        );
    }

    pub fn set_time_lock(env: Env, owner: Address, goal_id: u32, unlock_date: u64) {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::SET_TIME_LOCK);
        Self::extend_instance_ttl(&env);
        let mut goal =
            Self::get_goal_data(&env, goal_id).unwrap_or_else(|| panic!("Goal not found"));
        if goal.owner != owner {
            panic!("Unauthorized");
        }
        goal.unlock_date = Some(unlock_date);
        Self::set_goal_data(&env, goal_id, &goal);
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Low,
            symbol_short!("timelock"),
            (goal_id, owner, unlock_date),
        );
    }

    pub fn is_goal_completed(env: Env, goal_id: u32) -> bool {
        if let Some(goal) = Self::get_goal_data(&env, goal_id) {
            goal.current_amount >= goal.target_amount
        } else {
            false
        }
    }

    pub fn get_goal(env: Env, goal_id: u32) -> Option<SavingsGoal> {
        Self::get_goal_data(&env, goal_id)
    }

    pub fn get_all_goals(env: Env, owner: Address) -> Vec<SavingsGoal> {
        let ids = Self::get_owner_goal_ids(&env, &owner);
        let mut res = Vec::new(&env);
        for id in ids.iter() {
            if let Some(g) = Self::get_goal_data(&env, id) {
                res.push_back(g);
            }
        }
        res
    }

    pub fn get_goals(env: Env, owner: Address, cursor: u32, limit: u32) -> GoalPage {
        let ids = Self::get_owner_goal_ids(&env, &owner);
        let limit = Self::clamp_limit(limit);
        if ids.is_empty() {
            return GoalPage {
                items: Vec::new(&env),
                next_cursor: 0,
                count: 0,
            };
        }

        let mut start = 0u32;
        if cursor != 0 {
            let mut found = false;
            for i in 0..ids.len() {
                if ids.get(i) == Some(cursor) {
                    start = i + 1;
                    found = true;
                    break;
                }
            }
            if !found {
                panic!("Cursor not found");
            }
        }
<<<<<<< HEAD
        let end = (start + limit).min(ids.len());
        let mut items = Vec::new(&env);
        for i in start..end {
            if let Some(id) = ids.get(i) {
                if let Some(g) = Self::get_goal_data(&env, id) {
                    items.push_back(g);
=======

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
>>>>>>> origin/main
                }
            }
        }
        GoalPage {
            items,
            next_cursor: if end < ids.len() {
                ids.get(end - 1).unwrap_or(0)
            } else {
                0
            },
            count: end - start,
        }
    }

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
            panic!("Amount required");
        }
        if next_due <= env.ledger().timestamp() {
            panic!("Future required");
        }

        let s = env.storage().instance();
        let mut next_id: u32 = s.get(&symbol_short!("NEXT_SCH")).unwrap_or(0);
        next_id += 1;
        s.set(&symbol_short!("NEXT_SCH"), &next_id);

        let sch = SavingsSchedule {
            id: next_id,
            owner: owner.clone(),
            goal_id,
            amount,
            next_due,
            interval,
            recurring: interval > 0,
            active: true,
            created_at: env.ledger().timestamp(),
            last_executed: None,
            missed_count: 0,
        };
        Self::set_schedule_data(&env, next_id, &sch);
        Self::append_to_owner_schedule_ids(&env, &owner, next_id);
        Self::add_to_active_schedules(&env, next_id);

        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("sch_new"),
            (next_id, owner.clone()),
        );
        Self::extend_instance_ttl(&env);
        next_id
    }

    pub fn modify_savings_schedule(
        env: Env,
        owner: Address,
        schedule_id: u32,
        amount: i128,
        next_due: u64,
        interval: u64,
    ) {
        owner.require_auth();
        Self::extend_instance_ttl(&env);
        let mut s =
            Self::get_schedule_data(&env, schedule_id).unwrap_or_else(|| panic!("Not found"));
        if s.owner != owner {
            panic!("Unauthorized");
        }
        s.amount = amount;
        s.next_due = next_due;
        s.interval = interval;
        s.recurring = interval > 0;
        Self::set_schedule_data(&env, schedule_id, &s);
    }

    pub fn cancel_savings_schedule(env: Env, owner: Address, schedule_id: u32) {
        owner.require_auth();
        Self::extend_instance_ttl(&env);
        let mut s =
            Self::get_schedule_data(&env, schedule_id).unwrap_or_else(|| panic!("Not found"));
        if s.owner != owner {
            panic!("Unauthorized");
        }
        s.active = false;
        Self::set_schedule_data(&env, schedule_id, &s);
    }

    pub fn get_savings_schedule(env: Env, schedule_id: u32) -> Option<SavingsSchedule> {
        Self::get_schedule_data(&env, schedule_id)
    }

    pub fn execute_due_savings_schedules(env: Env) -> Vec<u32> {
        let now = env.ledger().timestamp();
        Self::extend_instance_ttl(&env);
        let active_ids = Self::get_active_schedules(&env);
        let mut executed = Vec::new(&env);
        let mut still_active = Vec::new(&env);

        for i in 0..active_ids.len() {
            let Some(id) = active_ids.get(i) else {
                continue;
            };
            let mut s = match Self::get_schedule_data(&env, id) {
                Some(x) => x,
                None => continue,
            };

            if !s.active {
                continue;
            }
            if s.next_due > now {
                still_active.push_back(id);
                continue;
            }

<<<<<<< HEAD
            if let Some(mut g) = Self::get_goal_data(&env, s.goal_id) {
                g.current_amount = g
                    .current_amount
                    .checked_add(s.amount)
                    .unwrap_or_else(|| panic!("Overflow"));
                let goal_key = (symbol_short!("GOAL_D"), s.goal_id);
                env.storage().persistent().set(&goal_key, &g);
                RemitwiseEvents::emit(
                    &env,
                    EventCategory::Transaction,
                    EventPriority::Medium,
                    symbol_short!("funds_add"),
                    (s.goal_id, g.owner, s.amount),
=======
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
>>>>>>> origin/main
                );
            }

            s.last_executed = Some(now);
            if s.recurring && s.interval > 0 {
                let mut next = s.next_due + s.interval;
                while next <= now {
                    s.missed_count += 1;
                    next += s.interval;
                }
                s.next_due = next;
                still_active.push_back(id);
            } else {
                s.active = false;
            }

            let sch_key = (symbol_short!("SCH_D"), id);
            env.storage().persistent().set(&sch_key, &s);
            executed.push_back(id);
            RemitwiseEvents::emit(
                &env,
                EventCategory::Transaction,
                EventPriority::Medium,
                symbol_short!("sch_exec"),
                id,
            );
        }

        env.storage()
            .instance()
            .set(&symbol_short!("ACT_SCH"), &still_active);
        executed
    }

    pub fn get_savings_schedules(env: Env, owner: Address) -> Vec<SavingsSchedule> {
        let ids = Self::get_owner_schedule_ids(&env, &owner);
        let mut res = Vec::new(&env);
        for id in ids.iter() {
            if let Some(s) = Self::get_schedule_data(&env, id) {
                res.push_back(s);
            }
        }
        res
    }

    pub fn export_snapshot(env: Env, owner: Address) -> GoalsExportSnapshot {
        let goals = Self::get_all_goals(env.clone(), owner);
        let next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0);
        let mut checksum = 0u64;
        for g in goals.iter() {
            checksum = checksum
                .wrapping_add(g.id as u64)
                .wrapping_add(g.current_amount as u64);
        }
        GoalsExportSnapshot {
            schema_version: SCHEMA_VERSION,
            checksum,
            next_id,
            goals,
        }
    }

    pub fn import_snapshot(
        env: Env,
        owner: Address,
        next_id: u32,
        snapshot: GoalsExportSnapshot,
    ) -> Result<bool, SavingsGoalError> {
        owner.require_auth();
        if snapshot.schema_version < MIN_SUPPORTED_SCHEMA_VERSION
            || snapshot.schema_version > SCHEMA_VERSION
        {
            return Err(SavingsGoalError::UnsupportedVersion);
        }
        let mut calc_checksum = 0u64;
        for g in snapshot.goals.iter() {
            calc_checksum = calc_checksum
                .wrapping_add(g.id as u64)
                .wrapping_add(g.current_amount as u64);
        }
        if calc_checksum != snapshot.checksum {
            return Err(SavingsGoalError::ChecksumMismatch);
        }

        for g in snapshot.goals.iter() {
            if g.owner != owner {
                return Err(SavingsGoalError::Unauthorized);
            }
            Self::set_goal_data(&env, g.id, &g);
            Self::append_to_owner_goal_ids(&env, &owner, g.id);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);
        Self::extend_instance_ttl(&env);
        Ok(true)
    }

    // -----------------------------------------------------------------------
    // Storage Helpers (Scalable)
    // -----------------------------------------------------------------------

    fn get_goal_data(env: &Env, id: u32) -> Option<SavingsGoal> {
        let key = (symbol_short!("GOAL_D"), id);
        env.storage().persistent().get(&key)
    }
    fn set_goal_data(env: &Env, id: u32, goal: &SavingsGoal) {
        let key = (symbol_short!("GOAL_D"), id);
        env.storage().persistent().set(&key, goal);
        Self::extend_persistent_ttl(env, &key);
    }
    fn get_schedule_data(env: &Env, id: u32) -> Option<SavingsSchedule> {
        let key = (symbol_short!("SCH_D"), id);
        env.storage().persistent().get(&key)
    }
    fn set_schedule_data(env: &Env, id: u32, schedule: &SavingsSchedule) {
        let key = (symbol_short!("SCH_D"), id);
        env.storage().persistent().set(&key, schedule);
        Self::extend_persistent_ttl(env, &key);
    }
    fn get_owner_goal_ids(env: &Env, owner: &Address) -> Vec<u32> {
        let key = (symbol_short!("O_GIDS"), owner.clone());
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env))
    }
    fn append_to_owner_goal_ids(env: &Env, owner: &Address, id: u32) {
        let key = (symbol_short!("O_GIDS"), owner.clone());
        let mut ids = Self::get_owner_goal_ids(env, owner);
        let mut exists = false;
        for existing in ids.iter() {
            if existing == id {
                exists = true;
                break;
            }
        }
        if !exists {
            ids.push_back(id);
            env.storage().persistent().set(&key, &ids);
            Self::extend_persistent_ttl(env, &key);
        }
    }
    fn get_owner_schedule_ids(env: &Env, owner: &Address) -> Vec<u32> {
        let key = (symbol_short!("O_SIDS"), owner.clone());
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env))
    }
    fn append_to_owner_schedule_ids(env: &Env, owner: &Address, id: u32) {
        let key = (symbol_short!("O_SIDS"), owner.clone());
        let mut ids = Self::get_owner_schedule_ids(env, owner);
        ids.push_back(id);
        env.storage().persistent().set(&key, &ids);
        Self::extend_persistent_ttl(env, &key);
    }
    fn get_active_schedules(env: &Env) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&symbol_short!("ACT_SCH"))
            .unwrap_or_else(|| Vec::new(env))
    }
    fn add_to_active_schedules(env: &Env, id: u32) {
        let mut active = Self::get_active_schedules(env);
        active.push_back(id);
        env.storage()
            .instance()
            .set(&symbol_short!("ACT_SCH"), &active);
        Self::extend_instance_ttl(env);
    }
    fn append_audit(env: &Env, operation: Symbol, caller: &Address, success: bool) {
        let mut log: Vec<AuditEntry> = env
            .storage()
            .instance()
            .get(&symbol_short!("AUDIT"))
            .unwrap_or_else(|| Vec::new(env));
        if log.len() >= MAX_AUDIT_ENTRIES {
            log.remove(0);
        }
        log.push_back(AuditEntry {
            operation,
            caller: caller.clone(),
            timestamp: env.ledger().timestamp(),
            success,
        });
        env.storage().instance().set(&symbol_short!("AUDIT"), &log);
        Self::extend_instance_ttl(env);
    }
}
#[cfg(test)]
mod test;
