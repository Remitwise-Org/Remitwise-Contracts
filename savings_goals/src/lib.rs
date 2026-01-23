
#![no_std]
use soroban_sdk::{contract, contractimpl, contracterror, contracttype, symbol_short, Env, Map, String, Symbol, Vec};

// ============================================================================
// : Use contracterror instead of Symbol for error handling
// Soroban contracts can't return Result<T, Symbol> from contract functions
// Must use a custom error enum with #[contracterror]
// ============================================================================
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SavingsError {
    InvalidAmount = 1,
    GoalNotFound = 2,
    GoalLocked = 3,
    InsufficientFunds = 4,
}

// ============================================================================
//  Optimized struct layout with field ordering
// - Changed id from u32 to u64 (native word size)
// - Ordered fields: large → medium → small → variable
// - Grouped related fields together
// ============================================================================
#[derive(Clone)]
#[contracttype]
#[derive(Clone)]
pub struct SavingsGoal {
    pub id: u64,                // Changed from u32 to u64 (native word size)
    pub target_amount: i128,    // Large values first
    pub current_amount: i128,
    pub target_date: u64,       // Fixed size fields
    pub locked: bool,           // Boolean
    pub name: String,           // Variable size last
}

// ============================================================================
//  Goal status enum for better state tracking
// - Provides clear state representation
// ============================================================================
#[derive(Clone, PartialEq)]
#[contracttype]
pub enum GoalStatus {
    Active,
    Completed,
    Expired,
    Locked,
}

// ============================================================================
//  Progress summary struct
// - Avoids recalculating progress multiple times
// - Bundles related data together
// ============================================================================
#[derive(Clone)]
#[contracttype]
pub struct GoalProgress {
    pub goal_id: u64,
    pub progress_percentage: u32,
    pub remaining_amount: i128,
    pub is_completed: bool,
    pub days_remaining: i64,
}

// ============================================================================
e
// Use a custom struct to wrap success/error state
// ============================================================================
#[derive(Clone)]
#[contracttype]
pub struct BatchResult {
    pub success: bool,
    pub amount: i128,
    pub error_code: u32,  // 0 = no error
}

// ============================================================================
//  Storage keys as constants
// - Eliminates repeated symbol creation
// - Created once at compile time
// ============================================================================
const GOALS_KEY: Symbol = symbol_short!("GOALS");
const NEXT_ID_KEY: Symbol = symbol_short!("NEXT_ID");
const TOTAL_SAVED_KEY: Symbol = symbol_short!("TOTAL");
const ACTIVE_COUNT_KEY: Symbol = symbol_short!("ACTIVE");
const COMPLETED_COUNT_KEY: Symbol = symbol_short!("COMPLETE");

#[contract]
pub struct SavingsGoals;

#[contractimpl]
impl SavingsGoals {
    // ========================================================================
    //  Initialize function
    // - Sets up all storage structures upfront
    // - Prevents repeated unwrap_or_else calls
    // ========================================================================
    pub fn initialize(env: Env) {
        if env.storage().instance().has(&NEXT_ID_KEY) {
            panic!("Already initialized");
        }
        
        env.storage().instance().set(&NEXT_ID_KEY, &0u64);
        env.storage().instance().set(&TOTAL_SAVED_KEY, &0i128);
        env.storage().instance().set(&ACTIVE_COUNT_KEY, &0u64);
        env.storage().instance().set(&COMPLETED_COUNT_KEY, &0u64);
        
        let goals: Map<u64, SavingsGoal> = Map::new(&env);
        env.storage().instance().set(&GOALS_KEY, &goals);
    }

    // ========================================================================
    //  Optimized create_goal function
    // - Early validation before storage reads
    // - Removed unnecessary clone on name
    // - Batch storage writes at end
    // - Updates cached counters
    // - Returns u64 instead of u32
    // ========================================================================
    pub fn create_goal(
        env: Env,
        name: String,
        target_amount: i128,
        target_date: u64,
    ) -> u64 {
        //  Early validation
        if target_amount <= 0 {
            panic!("Target amount must be positive");
        }

        let current_time = env.ledger().timestamp();
        if target_date <= current_time {
            panic!("Target date must be in the future");
        }

        // Single storage read
        let mut goals: Map<u64, SavingsGoal> = env
            .storage()
            .instance()
            .get(&GOALS_KEY)
            .unwrap_or_else(|| Map::new(&env));

        // Read and increment
        let next_id: u64 = env
            .storage()
            .instance()
            .get(&NEXT_ID_KEY)
            .unwrap_or(0u64)
            + 1;

        //  No clone on name (ownership transferred)
        let goal = SavingsGoal {
            id: next_id,
            target_amount,
            current_amount: 0,
            target_date,
            locked: true,
            name,
        };

        goals.set(next_id, goal);

        //Update active count
        let active_count: u64 = env
            .storage()
            .instance()
            .get(&ACTIVE_COUNT_KEY)
            .unwrap_or(0u64)
            + 1;

        //  Batch storage writes
        env.storage().instance().set(&GOALS_KEY, &goals);
        env.storage().instance().set(&NEXT_ID_KEY, &next_id);
        env.storage().instance().set(&ACTIVE_COUNT_KEY, &active_count);

        //  Extend TTL
        env.storage().instance().extend_ttl(100, 100);

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("savings"), SavingsEvent::GoalCreated),
            (next_id, owner),
        );

        next_id
    }

    // ========================================================================
    // uses custom error enum instead of Symbol for add_to_goal
    // - Type-safe error handling
    // - Proper Soroban error pattern
    // ========================================================================
    pub fn add_to_goal(env: Env, goal_id: u64, amount: i128) -> Result<i128, SavingsError> {
        if amount <= 0 {
            return Err(SavingsError::InvalidAmount);
        }

        let mut goals: Map<u64, SavingsGoal> = env
            .storage()
            .instance()
            .get(&GOALS_KEY)
            .unwrap_or_else(|| Map::new(&env));

        //  Check existence first
        if !goals.contains_key(goal_id) {
            return Err(SavingsError::GoalNotFound);
        }

        let mut goal = goals.get(goal_id).unwrap();

        // Track if goal becomes completed
        let was_completed = goal.current_amount >= goal.target_amount;

        goal.current_amount += amount;
        goals.set(goal_id, goal.clone());

        //  Update total saved
        let total_saved: i128 = env
            .storage()
            .instance()
            .get(&TOTAL_SAVED_KEY)
            .unwrap_or(0i128)
            + amount;

        env.storage().instance().set(&TOTAL_SAVED_KEY, &total_saved);

        // OPTIMIZATION: Update completed count if goal just completed
        let is_completed = goal.current_amount >= goal.target_amount;
        if !was_completed && is_completed {
            let completed_count: u64 = env
                .storage()
                .instance()
                .get(&COMPLETED_COUNT_KEY)
                .unwrap_or(0u64)
                + 1;
            
            let active_count: u64 = env
                .storage()
                .instance()
                .get(&ACTIVE_COUNT_KEY)
                .unwrap_or(1u64)
                .saturating_sub(1);

            env.storage().instance().set(&COMPLETED_COUNT_KEY, &completed_count);
            env.storage().instance().set(&ACTIVE_COUNT_KEY, &active_count);
        }

        env.storage().instance().set(&GOALS_KEY, &goals);
        env.storage().instance().extend_ttl(100, 100);

        Ok(goal.current_amount)
    }

    // ========================================================================
     //Direct access pattern for get_goal
    // - Uses functional chaining
    // ========================================================================
    pub fn get_goal(env: Env, goal_id: u64) -> Option<SavingsGoal> {
        env.storage()
            .instance()
            .get(&GOALS_KEY)
            .and_then(|goals: Map<u64, SavingsGoal>| goals.get(goal_id))
    }

    // ========================================================================
    // - Streamlined iteration
    // - Single NEXT_ID read
    // ========================================================================
    pub fn get_all_goals(env: Env) -> Vec<SavingsGoal> {
        let goals: Map<u64, SavingsGoal> = env
            .storage()
            .instance()
            .get(&GOALS_KEY)
            .unwrap_or_else(|| Map::new(&env));

        let max_id: u64 = env
            .storage()
            .instance()
            .get(&NEXT_ID_KEY)
            .unwrap_or(0u64);

        let mut result = Vec::new(&env);

        for id in 1..=max_id {
            if let Some(goal) = goals.get(id) {
                result.push_back(goal);
            }
        }

        result
    }

    // ========================================================================
    // - Single function call instead of two
    // - Direct comparison
    // ========================================================================
    pub fn is_goal_completed(env: Env, goal_id: u64) -> bool {
        env.storage()
            .instance()
            .get(&GOALS_KEY)
            .and_then(|goals: Map<u64, SavingsGoal>| goals.get(goal_id))
            .map(|goal| goal.current_amount >= goal.target_amount)
            .unwrap_or(false)
    }

}

#[cfg(test)]
mod test;
