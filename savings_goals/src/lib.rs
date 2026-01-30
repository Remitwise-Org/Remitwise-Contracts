#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Symbol, Vec,
};

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

/// Savings goal data structure with owner tracking for access control
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
}

/// Events emitted by the contract for audit trail
#[contracttype]
#[derive(Clone)]
pub enum SavingsEvent {
    GoalCreated,
    FundsAdded,
    FundsWithdrawn,
    GoalCompleted,
    GoalLocked,
    GoalUnlocked,
}

/// Snapshot for goals export/import (migration). Checksum is numeric for on-chain verification.
#[contracttype]
#[derive(Clone)]
pub struct GoalsExportSnapshot {
    pub version: u32,
    pub checksum: u64,
    pub next_id: u32,
    pub goals: Vec<SavingsGoal>,
}

/// Audit log entry for security and compliance.
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

#[contractimpl]
impl SavingsGoalContract {
    // Storage keys
    const STORAGE_NEXT_ID: Symbol = symbol_short!("NEXT_ID");
    const STORAGE_GOALS: Symbol = symbol_short!("GOALS");

    /// Initialize contract storage
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

    /// Create a new savings goal
    ///
    /// # Arguments
    /// * `owner` - Address of the goal owner (must authorize)
    /// * `name` - Name of the goal (e.g., "Education", "Medical")
    /// * `target_amount` - Target amount to save (must be positive)
    /// * `target_date` - Target date as Unix timestamp
    ///
    /// # Returns
    /// The ID of the created goal
    ///
    /// # Panics
    /// - If owner doesn't authorize the transaction
    /// - If target_amount is not positive
    pub fn create_goal(
        env: Env,
        owner: Address,
        name: String,
        target_amount: i128,
        target_date: u64,
    ) -> u32 {
        // Access control: require owner authorization
        owner.require_auth();

        // Input validation
        if target_amount <= 0 {
            Self::append_audit(&env, symbol_short!("create"), &owner, false);
            panic!("Target amount must be positive");
        }

        // Extend storage TTL
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
            name,
            target_amount,
            current_amount: 0,
            target_date,
            locked: true,
        };

        goals.set(next_id, goal);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);

        Self::append_audit(&env, symbol_short!("create"), &owner, true);
        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::GoalCreated),
            (next_id, owner),
        );

        next_id
    }

    /// Add funds to a savings goal
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the goal owner)
    /// * `goal_id` - ID of the goal
    /// * `amount` - Amount to add (must be positive)
    ///
    /// # Returns
    /// Updated current amount
    ///
    /// # Panics
    /// - If caller is not the goal owner
    /// - If goal is not found
    /// - If amount is not positive
    pub fn add_to_goal(env: Env, caller: Address, goal_id: u32, amount: i128) -> i128 {
        // Access control: require caller authorization
        caller.require_auth();

        // Input validation
        if amount <= 0 {
            Self::append_audit(&env, symbol_short!("add"), &caller, false);
            panic!("Amount must be positive");
        }

        // Extend storage TTL
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
                panic!("Goal not found");
            }
        };

        // Access control: verify caller is the owner
        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("add"), &caller, false);
            panic!("Only the goal owner can add funds");
        }

        goal.current_amount = goal.current_amount.checked_add(amount).expect("overflow");
        let new_amount = goal.current_amount;
        let is_completed = goal.current_amount >= goal.target_amount;
        let goal_owner = goal.owner.clone();

        goals.set(goal_id, goal);
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);

        Self::append_audit(&env, symbol_short!("add"), &caller, true);
        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::FundsAdded),
            (goal_id, goal_owner.clone(), amount),
        );

        // Emit completion event if goal is now complete
        if is_completed {
            env.events().publish(
                (symbol_short!("savings"), SavingsEvent::GoalCompleted),
                (goal_id, goal_owner),
            );
        }

        new_amount
    }

    /// Withdraw funds from a savings goal
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the goal owner)
    /// * `goal_id` - ID of the goal
    /// * `amount` - Amount to withdraw (must be positive and <= current_amount)
    ///
    /// # Returns
    /// Updated current amount
    ///
    /// # Panics
    /// - If caller is not the goal owner
    /// - If goal is not found
    /// - If goal is locked
    /// - If amount is not positive
    /// - If amount exceeds current balance
    pub fn withdraw_from_goal(env: Env, caller: Address, goal_id: u32, amount: i128) -> i128 {
        // Access control: require caller authorization
        caller.require_auth();

        // Input validation
        if amount <= 0 {
            Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
            panic!("Amount must be positive");
        }

        // Extend storage TTL
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
                panic!("Goal not found");
            }
        };

        // Access control: verify caller is the owner
        if goal.owner != caller {
            Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
            panic!("Only the goal owner can withdraw funds");
        }

        // Check if goal is locked
        if goal.locked {
            Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
            panic!("Cannot withdraw from a locked goal");
        }

        // Check sufficient balance
        if amount > goal.current_amount {
            Self::append_audit(&env, symbol_short!("withdraw"), &caller, false);
            panic!("Insufficient balance");
        }

        goal.current_amount = goal.current_amount.checked_sub(amount).expect("underflow");
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

        new_amount
    }

    /// Lock a savings goal (prevent withdrawals)
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the goal owner)
    /// * `goal_id` - ID of the goal
    ///
    /// # Panics
    /// - If caller is not the goal owner
    /// - If goal is not found
    pub fn lock_goal(env: Env, caller: Address, goal_id: u32) -> bool {
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

    /// Unlock a savings goal (allow withdrawals)
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the goal owner)
    /// * `goal_id` - ID of the goal
    ///
    /// # Panics
    /// - If caller is not the goal owner
    /// - If goal is not found
    pub fn unlock_goal(env: Env, caller: Address, goal_id: u32) -> bool {
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

    /// Get a savings goal by ID
    ///
    /// # Arguments
    /// * `goal_id` - ID of the goal
    ///
    /// # Returns
    /// SavingsGoal struct or None if not found
    pub fn get_goal(env: Env, goal_id: u32) -> Option<SavingsGoal> {
        let goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        goals.get(goal_id)
    }

    /// Get all savings goals for a specific owner
    ///
    /// # Arguments
    /// * `owner` - Address of the goal owner
    ///
    /// # Returns
    /// Vec of all SavingsGoal structs belonging to the owner
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

    /// Check if a goal is completed
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

    /// Get current nonce for an address (for import_snapshot replay protection).
    pub fn get_nonce(env: Env, address: Address) -> u64 {
        let nonces: Option<Map<Address, u64>> =
            env.storage().instance().get(&symbol_short!("NONCES"));
        nonces.as_ref().and_then(|m| m.get(address)).unwrap_or(0)
    }

    /// Export all goals as snapshot for backup/migration.
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

    /// Import snapshot (full restore). Validates version and checksum. Requires nonce for replay protection.
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
        for g in snapshot.goals.iter() {
            goals.set(g.id, g);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("GOALS"), &goals);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &snapshot.next_id);

        Self::increment_nonce(&env, &caller);
        Self::append_audit(&env, symbol_short!("import"), &caller, true);
        true
    }

    /// Return recent audit log entries.
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

    /// Extend the TTL of instance storage
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }
}

#[cfg(test)]
mod test;
