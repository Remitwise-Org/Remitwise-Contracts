#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Symbol, Vec,
};

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

const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280;
const INSTANCE_BUMP_AMOUNT: u32 = 518400;

/// Pagination constants
pub const DEFAULT_PAGE_LIMIT: u32 = 20;
pub const MAX_PAGE_LIMIT: u32 = 50;

#[contract]
pub struct SavingsGoalContract;

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
#[derive(Clone, Copy)]
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

#[contracttype]
#[derive(Clone)]
pub struct GoalsExportSnapshot {
    pub version: u32,
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

const SNAPSHOT_VERSION: u32 = 1;
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
        let admin = Self::get_pause_admin(&env).expect("No pause admin set");
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
        let admin = Self::get_pause_admin(&env).expect("No pause admin set");
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
        let admin = Self::get_pause_admin(&env).expect("No pause admin set");
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
        let admin = Self::get_pause_admin(&env).expect("No pause admin set");
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

    pub fn set_upgrade_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let current = Self::get_upgrade_admin(&env);
        match current {
            None => {
                if caller != new_admin {
                    panic!("Unauthorized");
                }
            }
            Some(adm) if adm != caller => panic!("Unauthorized"),
            _ => {}
        }
        env.storage()
            .instance()
            .set(&symbol_short!("UPG_ADM"), &new_admin);
    }

    pub fn set_version(env: Env, caller: Address, new_version: u32) {
        caller.require_auth();
        let admin = Self::get_upgrade_admin(&env).expect("No upgrade admin set");
        if admin != caller {
            panic!("Unauthorized");
        }
        let prev = Self::get_version(env.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("VERSION"), &new_version);
        env.events().publish(
            (symbol_short!("savings"), symbol_short!("upgraded")),
            (prev, new_version),
        );
    }

    // -----------------------------------------------------------------------
    // Core goal operations
    // -----------------------------------------------------------------------

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
        env.events().publish((GOAL_CREATED,), event);
        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::GoalCreated),
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

        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("add"), &caller, false);
            return Err(SavingsGoalsError::Unauthorized);
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
        env.events().publish((FUNDS_ADDED,), funds_event);

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

        if was_completed {
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
    ) -> u32 {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::ADD_TO_GOAL);
        if contributions.len() > MAX_BATCH_SIZE {
            panic!("Batch too large");
        }
        let goals_map: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));
        for item in contributions.iter() {
            if item.amount <= 0 {
                panic!("Amount must be positive");
            }
            let goal = goals_map.get(item.goal_id).expect("Goal not found");
            if goal.owner != caller {
                panic!("Not owner of all goals");
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
            let mut goal = goals.get(item.goal_id).expect("Goal not found");
            if goal.owner != caller {
                panic!("Batch validation failed");
            }
            goal.current_amount = goal
                .current_amount
                .checked_add(item.amount)
                .expect("overflow");
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
            env.events().publish((FUNDS_ADDED,), funds_event);
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
            if was_completed {
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
        env.events().publish(
            (symbol_short!("savings"), symbol_short!("batch_add")),
            (count, caller),
        );
        count
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

    /// Get a page of savings goals for `owner`.
    ///
    /// # Arguments
    /// * `owner`  – whose goals to return
    /// * `cursor` – start after this goal ID (pass 0 for the first page)
    /// * `limit`  – max items per page (0 → DEFAULT_PAGE_LIMIT, capped at MAX_PAGE_LIMIT)
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

        let mut result = Vec::new(&env);
        let mut next_cursor: u32 = 0;
        let mut collected: u32 = 0;

        for (id, goal) in goals.iter() {
            if id <= cursor {
                continue;
            }
            if goal.owner != owner {
                continue;
            }
            if collected < limit {
                result.push_back(goal);
                collected += 1;
                next_cursor = id; // track last returned ID
            } else {
                break;
            }
        }

        // If we didn't fill the page, there are no more items
        if collected < limit {
            next_cursor = 0;
        }

        GoalPage {
            items: result,
            next_cursor,
            count: collected,
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
        let checksum = Self::compute_goals_checksum(SNAPSHOT_VERSION, next_id, &list);
        GoalsExportSnapshot {
            version: SNAPSHOT_VERSION,
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
    ) -> bool {
        caller.require_auth();
        Self::require_nonce(&env, &caller, nonce);

        if snapshot.version != SNAPSHOT_VERSION {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            panic!("Unsupported snapshot version");
        }
        let expected =
            Self::compute_goals_checksum(snapshot.version, snapshot.next_id, &snapshot.goals);
        if snapshot.checksum != expected {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            panic!("Snapshot checksum mismatch");
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
        true
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
        let next = current.checked_add(1).expect("nonce overflow");
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

        let goal = goals.get(goal_id).expect("Goal not found");

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

        let mut schedule = schedules.get(schedule_id).expect("Schedule not found");

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

        let mut schedule = schedules.get(schedule_id).expect("Schedule not found");

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

            if let Some(mut goal) = goals.get(schedule.goal_id) {
                goal.current_amount = goal
                    .current_amount
                    .checked_add(schedule.amount)
                    .expect("overflow");

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
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, String};

    fn make_env() -> Env {
        Env::default()
    }

    fn setup_goals(env: &Env, client: &SavingsGoalContractClient, owner: &Address, count: u32) {
        for i in 0..count {
            client.create_goal(
                owner,
                &String::from_str(env, "Goal"),
                &(1000i128 * (i as i128 + 1)),
                &(env.ledger().timestamp() + 86400 * (i as u64 + 1)),
            );
        }
    }

    // --- get_goals ---

    #[test]
    fn test_get_goals_empty() {
        let env = make_env();
        env.mock_all_auths();
        let id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &id);
        let owner = Address::generate(&env);

        let page = client.get_goals(&owner, &0, &0);
        assert_eq!(page.count, 0);
        assert_eq!(page.next_cursor, 0);
        assert_eq!(page.items.len(), 0);
    }

    #[test]
    fn test_get_goals_single_page() {
        let env = make_env();
        env.mock_all_auths();
        let id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &id);
        let owner = Address::generate(&env);

        setup_goals(&env, &client, &owner, 5);

        let page = client.get_goals(&owner, &0, &10);
        assert_eq!(page.count, 5);
        assert_eq!(page.next_cursor, 0);
    }

    #[test]
    fn test_get_goals_multiple_pages() {
        let env = make_env();
        env.mock_all_auths();
        let id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &id);
        let owner = Address::generate(&env);

        setup_goals(&env, &client, &owner, 9);

        // Page 1
        let page1 = client.get_goals(&owner, &0, &4);
        assert_eq!(page1.count, 4);
        assert!(page1.next_cursor > 0);

        // Page 2
        let page2 = client.get_goals(&owner, &page1.next_cursor, &4);
        assert_eq!(page2.count, 4);
        assert!(page2.next_cursor > 0);

        // Page 3 (last)
        let page3 = client.get_goals(&owner, &page2.next_cursor, &4);
        assert_eq!(page3.count, 1);
        assert_eq!(page3.next_cursor, 0);
    }

    #[test]
    fn test_get_goals_multi_owner_isolation() {
        let env = make_env();
        env.mock_all_auths();
        let id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &id);
        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);

        setup_goals(&env, &client, &owner_a, 3);
        setup_goals(&env, &client, &owner_b, 4);

        let page_a = client.get_goals(&owner_a, &0, &20);
        assert_eq!(page_a.count, 3);
        for g in page_a.items.iter() {
            assert_eq!(g.owner, owner_a);
        }

        let page_b = client.get_goals(&owner_b, &0, &20);
        assert_eq!(page_b.count, 4);
    }

    #[test]
    fn test_get_goals_cursor_is_exclusive() {
        let env = make_env();
        env.mock_all_auths();
        let id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &id);
        let owner = Address::generate(&env);

        setup_goals(&env, &client, &owner, 4);

        let first = client.get_goals(&owner, &0, &2);
        assert_eq!(first.count, 2);
        let last_id = first.items.get(1).unwrap().id;

        // cursor should be exclusive — next page should NOT include `last_id`
        let second = client.get_goals(&owner, &last_id, &2);
        for g in second.items.iter() {
            assert!(g.id > last_id, "cursor should be exclusive");
        }
    }

    #[test]
    fn test_limit_zero_uses_default() {
        let env = make_env();
        env.mock_all_auths();
        let id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &id);
        let owner = Address::generate(&env);

        setup_goals(&env, &client, &owner, 3);
        let page = client.get_goals(&owner, &0, &0);
        assert_eq!(page.count, 3); // 3 < DEFAULT_PAGE_LIMIT so all returned
    }

    #[test]
    fn test_get_all_goals_backward_compat() {
        let env = make_env();
        env.mock_all_auths();
        let id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &id);
        let owner = Address::generate(&env);

        setup_goals(&env, &client, &owner, 5);
        let all = client.get_all_goals(&owner);
        assert_eq!(all.len(), 5);
    }
}
