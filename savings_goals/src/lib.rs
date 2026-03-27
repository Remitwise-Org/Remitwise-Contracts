#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, String,
    Symbol, Vec,
};
use remitwise_common::{
    RemitwiseEvents, EventCategory, EventPriority, 
    INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT,
    PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT,
    DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT, MAX_BATCH_SIZE, CONTRACT_VERSION
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

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SavingsGoalError {
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
        if limit == 0 { DEFAULT_PAGE_LIMIT } else { limit.min(MAX_PAGE_LIMIT) }
    }

    fn require_not_paused(env: &Env, func: Symbol) {
        if env.storage().instance().get::<_, bool>(&symbol_short!("PAUSED")).unwrap_or(false) {
            panic!("Contract is paused");
        }
        let m: Map<Symbol, bool> = env.storage().instance().get(&symbol_short!("PAUSED_FN")).unwrap_or_else(|| Map::new(env));
        if m.get(func).unwrap_or(false) {
            panic!("Function is paused");
        }
    }

    fn extend_instance_ttl(env: &Env) {
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn extend_persistent_ttl(env: &Env, key: &impl soroban_sdk::IntoVal<Env, soroban_sdk::Val>) {
        env.storage().persistent().extend_ttl(key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    }

    // -----------------------------------------------------------------------
    // Contract Lifecycle
    // -----------------------------------------------------------------------

    pub fn init(env: Env) {
        let s = env.storage().instance();
        if !s.has(&symbol_short!("NEXT_ID")) { s.set(&symbol_short!("NEXT_ID"), &0u32); }
        if !s.has(&symbol_short!("NEXT_SCH")) { s.set(&symbol_short!("NEXT_SCH"), &0u32); }
        Self::extend_instance_ttl(&env);
    }

    pub fn set_pause_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let s = env.storage().instance();
        let current: Option<Address> = s.get(&symbol_short!("PAUSE_ADM"));
        if let Some(admin) = current {
            if admin != caller { panic!("Unauthorized"); }
        } else if caller != new_admin {
            panic!("Unauthorized");
        }
        s.set(&symbol_short!("PAUSE_ADM"), &new_admin);
        Self::extend_instance_ttl(&env);
    }

    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&symbol_short!("PAUSE_ADM")).expect("No admin");
        if admin != caller { panic!("Unauthorized"); }
        env.storage().instance().set(&symbol_short!("PAUSED"), &true);
        Self::extend_instance_ttl(&env);
    }

    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&symbol_short!("PAUSE_ADM")).expect("No admin");
        if admin != caller { panic!("Unauthorized"); }
        env.storage().instance().set(&symbol_short!("PAUSED"), &false);
        Self::extend_instance_ttl(&env);
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&symbol_short!("VERSION")).unwrap_or(CONTRACT_VERSION)
    }

    // -----------------------------------------------------------------------
    // Core Logic (Scalable Storage)
    // -----------------------------------------------------------------------

    pub fn create_goal(env: Env, owner: Address, name: String, target_amount: i128, target_date: u64) -> Result<u32, SavingsGoalError> {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::CREATE_GOAL);
        if target_amount <= 0 { 
            Self::append_audit(&env, symbol_short!("create"), &owner, false);
            return Err(SavingsGoalError::InvalidAmount); 
        }

        Self::extend_instance_ttl(&env);
        let mut next_id: u32 = env.storage().instance().get(&symbol_short!("NEXT_ID")).unwrap_or(0);
        next_id += 1;
        env.storage().instance().set(&symbol_short!("NEXT_ID"), &next_id);

        let goal = SavingsGoal { id: next_id, owner: owner.clone(), name: name.clone(), target_amount, current_amount: 0, target_date, locked: true, unlock_date: None, tags: Vec::new(&env) };
        Self::set_goal_data(&env, next_id, &goal);
        Self::append_to_owner_goal_ids(&env, &owner, next_id);

        RemitwiseEvents::emit(&env, EventCategory::State, EventPriority::Medium, GOAL_CREATED, GoalCreatedEvent { goal_id: next_id, name, target_amount, target_date, timestamp: env.ledger().timestamp() });
        Self::append_audit(&env, symbol_short!("create"), &owner, true);
        Ok(next_id)
    }

    pub fn add_to_goal(env: Env, caller: Address, goal_id: u32, amount: i128) -> Result<i128, SavingsGoalError> {
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

        goal.current_amount = goal.current_amount.checked_add(amount).ok_or(SavingsGoalError::Overflow)?;
        let was_completed = goal.current_amount >= goal.target_amount;
        let previously_completed = (goal.current_amount - amount) >= goal.target_amount;

        Self::set_goal_data(&env, goal_id, &goal);

        RemitwiseEvents::emit(&env, EventCategory::Transaction, EventPriority::Medium, FUNDS_ADDED, FundsAddedEvent { goal_id, amount, new_total: goal.current_amount, timestamp: env.ledger().timestamp() });
        if was_completed && !previously_completed {
            RemitwiseEvents::emit(&env, EventCategory::Transaction, EventPriority::High, GOAL_COMPLETED, GoalCompletedEvent { goal_id, name: goal.name, final_amount: goal.current_amount, timestamp: env.ledger().timestamp() });
        }
        RemitwiseEvents::emit(&env, EventCategory::Transaction, EventPriority::Medium, symbol_short!("funds_add"), (goal_id, caller.clone(), amount));
        Self::append_audit(&env, symbol_short!("add"), &caller, true);
        Ok(goal.current_amount)
    }

    pub fn batch_add_to_goals(env: Env, caller: Address, contributions: Vec<ContributionItem>) -> u32 {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::ADD_TO_GOAL);
        if contributions.len() > MAX_BATCH_SIZE { panic!("Batch too large"); }

        Self::extend_instance_ttl(&env);
        let mut count = 0u32;
        for item in contributions.iter() {
            if item.amount <= 0 { panic!("Amount must be positive"); }
            let mut goal = Self::get_goal_data(&env, item.goal_id).expect("Goal not found");
            if goal.owner != caller { panic!("Not owner"); }

            goal.current_amount = goal.current_amount.checked_add(item.amount).expect("Overflow");
            let key = (symbol_short!("GOAL_D"), item.goal_id);
            env.storage().persistent().set(&key, &goal);
            count += 1;
        }
        Self::append_audit(&env, symbol_short!("batch_add"), &caller, true);
        count
    }

    pub fn withdraw_from_goal(env: Env, caller: Address, goal_id: u32, amount: i128) -> Result<i128, SavingsGoalError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::WITHDRAW);
        Self::extend_instance_ttl(&env);
        if amount <= 0 { return Err(SavingsGoalError::InvalidAmount); }

        let mut goal = Self::get_goal_data(&env, goal_id).ok_or(SavingsGoalError::GoalNotFound)?;
        if goal.owner != caller { return Err(SavingsGoalError::Unauthorized); }
        if goal.locked { return Err(SavingsGoalError::GoalLocked); }
        if let Some(unlock) = goal.unlock_date {
            if env.ledger().timestamp() < unlock { return Err(SavingsGoalError::GoalLocked); }
        }
        if amount > goal.current_amount { return Err(SavingsGoalError::InsufficientBalance); }

        goal.current_amount = goal.current_amount.checked_sub(amount).ok_or(SavingsGoalError::Overflow)?;
        Self::set_goal_data(&env, goal_id, &goal);

        RemitwiseEvents::emit(&env, EventCategory::Transaction, EventPriority::Medium, symbol_short!("funds_wit"), (goal_id, caller.clone(), amount));
        Self::append_audit(&env, symbol_short!("withdraw"), &caller, true);
        Ok(goal.current_amount)
    }

    pub fn lock_goal(env: Env, owner: Address, goal_id: u32) {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::LOCK);
        Self::extend_instance_ttl(&env);
        let mut goal = Self::get_goal_data(&env, goal_id).expect("Goal not found");
        if goal.owner != owner { panic!("Unauthorized"); }
        goal.locked = true;
        Self::set_goal_data(&env, goal_id, &goal);
        RemitwiseEvents::emit(&env, EventCategory::State, EventPriority::Low, symbol_short!("locked"), (goal_id, owner));
    }

    pub fn unlock_goal(env: Env, owner: Address, goal_id: u32) {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::UNLOCK);
        Self::extend_instance_ttl(&env);
        let mut goal = Self::get_goal_data(&env, goal_id).expect("Goal not found");
        if goal.owner != owner { panic!("Unauthorized"); }
        goal.locked = false;
        Self::set_goal_data(&env, goal_id, &goal);
        RemitwiseEvents::emit(&env, EventCategory::State, EventPriority::Low, symbol_short!("unlocked"), (goal_id, owner));
    }

    pub fn set_time_lock(env: Env, owner: Address, goal_id: u32, unlock_date: u64) {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::SET_TIME_LOCK);
        Self::extend_instance_ttl(&env);
        let mut goal = Self::get_goal_data(&env, goal_id).expect("Goal not found");
        if goal.owner != owner { panic!("Unauthorized"); }
        goal.unlock_date = Some(unlock_date);
        Self::set_goal_data(&env, goal_id, &goal);
        RemitwiseEvents::emit(&env, EventCategory::State, EventPriority::Low, symbol_short!("timelock"), (goal_id, owner, unlock_date));
    }

    pub fn is_goal_completed(env: Env, goal_id: u32) -> bool {
        if let Some(goal) = Self::get_goal_data(&env, goal_id) {
            goal.current_amount >= goal.target_amount
        } else {
            false
        }
    }

    pub fn get_goal(env: Env, goal_id: u32) -> Option<SavingsGoal> { Self::get_goal_data(&env, goal_id) }

    pub fn get_all_goals(env: Env, owner: Address) -> Vec<SavingsGoal> {
        let ids = Self::get_owner_goal_ids(&env, &owner);
        let mut res = Vec::new(&env);
        for id in ids.iter() {
            if let Some(g) = Self::get_goal_data(&env, id) { res.push_back(g); }
        }
        res
    }

    pub fn get_goals(env: Env, owner: Address, cursor: u32, limit: u32) -> GoalPage {
        let ids = Self::get_owner_goal_ids(&env, &owner);
        let limit = Self::clamp_limit(limit);
        if ids.is_empty() { return GoalPage { items: Vec::new(&env), next_cursor: 0, count: 0 }; }

        let mut start = 0u32;
        if cursor != 0 {
            let mut found = false;
            for i in 0..ids.len() {
                if ids.get(i) == Some(cursor) { start = i + 1; found = true; break; }
            }
            if !found { panic!("Cursor not found"); }
        }
        let end = (start + limit).min(ids.len());
        let mut items = Vec::new(&env);
        for i in start..end {
            if let Some(id) = ids.get(i) {
                if let Some(g) = Self::get_goal_data(&env, id) { items.push_back(g); }
            }
        }
        GoalPage { items, next_cursor: if end < ids.len() { ids.get(end - 1).unwrap_or(0) } else { 0 }, count: end - start }
    }

    pub fn create_savings_schedule(env: Env, owner: Address, goal_id: u32, amount: i128, next_due: u64, interval: u64) -> u32 {
        owner.require_auth();
        if amount <= 0 { panic!("Amount required"); }
        if next_due <= env.ledger().timestamp() { panic!("Future required"); }

        let s = env.storage().instance();
        let mut next_id: u32 = s.get(&symbol_short!("NEXT_SCH")).unwrap_or(0);
        next_id += 1;
        s.set(&symbol_short!("NEXT_SCH"), &next_id);

        let sch = SavingsSchedule { id: next_id, owner: owner.clone(), goal_id, amount, next_due, interval, recurring: interval > 0, active: true, created_at: env.ledger().timestamp(), last_executed: None, missed_count: 0 };
        Self::set_schedule_data(&env, next_id, &sch);
        Self::append_to_owner_schedule_ids(&env, &owner, next_id);
        Self::add_to_active_schedules(&env, next_id);

        RemitwiseEvents::emit(&env, EventCategory::State, EventPriority::Medium, symbol_short!("sch_new"), (next_id, owner.clone()));
        Self::extend_instance_ttl(&env);
        next_id
    }

    pub fn modify_savings_schedule(env: Env, owner: Address, schedule_id: u32, amount: i128, next_due: u64, interval: u64) {
        owner.require_auth();
        Self::extend_instance_ttl(&env);
        let mut s = Self::get_schedule_data(&env, schedule_id).expect("Not found");
        if s.owner != owner { panic!("Unauthorized"); }
        s.amount = amount;
        s.next_due = next_due;
        s.interval = interval;
        s.recurring = interval > 0;
        Self::set_schedule_data(&env, schedule_id, &s);
    }

    pub fn cancel_savings_schedule(env: Env, owner: Address, schedule_id: u32) {
        owner.require_auth();
        Self::extend_instance_ttl(&env);
        let mut s = Self::get_schedule_data(&env, schedule_id).expect("Not found");
        if s.owner != owner { panic!("Unauthorized"); }
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
            let id = active_ids.get(i).unwrap();
            let mut s = match Self::get_schedule_data(&env, id) { Some(x) => x, None => continue };

            if !s.active { continue; }
            if s.next_due > now { 
                still_active.push_back(id);
                continue; 
            }

            if let Some(mut g) = Self::get_goal_data(&env, s.goal_id) {
                g.current_amount = g.current_amount.checked_add(s.amount).expect("Overflow");
                let goal_key = (symbol_short!("GOAL_D"), s.goal_id);
                env.storage().persistent().set(&goal_key, &g);
                RemitwiseEvents::emit(&env, EventCategory::Transaction, EventPriority::Medium, symbol_short!("funds_add"), (s.goal_id, g.owner, s.amount));
            }

            s.last_executed = Some(now);
            if s.recurring && s.interval > 0 {
                let mut next = s.next_due + s.interval;
                while next <= now { s.missed_count += 1; next += s.interval; }
                s.next_due = next;
                still_active.push_back(id);
            } else {
                s.active = false;
            }

            let sch_key = (symbol_short!("SCH_D"), id);
            env.storage().persistent().set(&sch_key, &s);
            executed.push_back(id);
            RemitwiseEvents::emit(&env, EventCategory::Transaction, EventPriority::Medium, symbol_short!("sch_exec"), id);
        }

        env.storage().instance().set(&symbol_short!("ACT_SCH"), &still_active);
        executed
    }

    pub fn get_savings_schedules(env: Env, owner: Address) -> Vec<SavingsSchedule> {
        let ids = Self::get_owner_schedule_ids(&env, &owner);
        let mut res = Vec::new(&env);
        for id in ids.iter() {
            if let Some(s) = Self::get_schedule_data(&env, id) { res.push_back(s); }
        }
        res
    }

    pub fn export_snapshot(env: Env, owner: Address) -> GoalsExportSnapshot {
        let goals = Self::get_all_goals(env.clone(), owner);
        let next_id = env.storage().instance().get(&symbol_short!("NEXT_ID")).unwrap_or(0);
        let mut checksum = 0u64;
        for g in goals.iter() {
            checksum = checksum.wrapping_add(g.id as u64).wrapping_add(g.current_amount as u64);
        }
        GoalsExportSnapshot { schema_version: SCHEMA_VERSION, checksum, next_id, goals }
    }

    pub fn import_snapshot(env: Env, owner: Address, next_id: u32, snapshot: GoalsExportSnapshot) -> Result<bool, SavingsGoalError> {
        owner.require_auth();
        if snapshot.schema_version < MIN_SUPPORTED_SCHEMA_VERSION || snapshot.schema_version > SCHEMA_VERSION {
            return Err(SavingsGoalError::UnsupportedVersion);
        }
        let mut calc_checksum = 0u64;
        for g in snapshot.goals.iter() {
            calc_checksum = calc_checksum.wrapping_add(g.id as u64).wrapping_add(g.current_amount as u64);
        }
        if calc_checksum != snapshot.checksum {
            return Err(SavingsGoalError::ChecksumMismatch);
        }

        for g in snapshot.goals.iter() {
            if g.owner != owner { return Err(SavingsGoalError::Unauthorized); }
            Self::set_goal_data(&env, g.id, &g);
            Self::append_to_owner_goal_ids(&env, &owner, g.id);
        }
        env.storage().instance().set(&symbol_short!("NEXT_ID"), &next_id);
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
        env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env))
    }
    fn append_to_owner_goal_ids(env: &Env, owner: &Address, id: u32) {
        let key = (symbol_short!("O_GIDS"), owner.clone());
        let mut ids = Self::get_owner_goal_ids(env, owner);
        let mut exists = false;
        for existing in ids.iter() { if existing == id { exists = true; break; } }
        if !exists {
            ids.push_back(id);
            env.storage().persistent().set(&key, &ids);
            Self::extend_persistent_ttl(env, &key);
        }
    }
    fn get_owner_schedule_ids(env: &Env, owner: &Address) -> Vec<u32> {
        let key = (symbol_short!("O_SIDS"), owner.clone());
        env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env))
    }
    fn append_to_owner_schedule_ids(env: &Env, owner: &Address, id: u32) {
        let key = (symbol_short!("O_SIDS"), owner.clone());
        let mut ids = Self::get_owner_schedule_ids(env, owner);
        ids.push_back(id);
        env.storage().persistent().set(&key, &ids);
        Self::extend_persistent_ttl(env, &key);
    }
    fn get_active_schedules(env: &Env) -> Vec<u32> { env.storage().instance().get(&symbol_short!("ACT_SCH")).unwrap_or_else(|| Vec::new(env)) }
    fn add_to_active_schedules(env: &Env, id: u32) {
        let mut active = Self::get_active_schedules(env);
        active.push_back(id);
        env.storage().instance().set(&symbol_short!("ACT_SCH"), &active);
        Self::extend_instance_ttl(env);
    }
    fn append_audit(env: &Env, operation: Symbol, caller: &Address, success: bool) {
        let mut log: Vec<AuditEntry> = env.storage().instance().get(&symbol_short!("AUDIT")).unwrap_or_else(|| Vec::new(env));
        if log.len() >= MAX_AUDIT_ENTRIES { log.remove(0); }
        log.push_back(AuditEntry { operation, caller: caller.clone(), timestamp: env.ledger().timestamp(), success });
        env.storage().instance().set(&symbol_short!("AUDIT"), &log);
        Self::extend_instance_ttl(env);
    }
}
mod test;
