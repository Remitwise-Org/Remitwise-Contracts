#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Env, Map, String, Vec};

#[derive(Clone)]
#[contracttype]
pub struct SavingsGoal {
    pub id: u32,
    pub name: String,
    pub target_amount: i128,
    pub current_amount: i128,
    pub target_date: u64, // Unix timestamp
    pub locked: bool,
}

#[contract]
pub struct SavingsGoals;

#[contractimpl]
impl SavingsGoals {
    /// Create a new savings goal
    ///
    /// # Arguments
    /// * `name` - Name of the goal (e.g., "Education", "Medical")
    /// * `target_amount` - Target amount to save (must be positive)
    /// * `target_date` - Target date as Unix timestamp
    ///
    /// # Returns
    /// The ID of the created goal. The goal starts with current_amount = 0 and is locked.
    ///
    /// # Errors
    /// This function does not return errors; it always succeeds by creating a new goal.
    pub fn create_goal(env: Env, name: String, target_amount: i128, target_date: u64) -> u32 {
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
            name: name.clone(),
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

        next_id
    }

    /// Add funds to a savings goal
    ///
    /// # Arguments
    /// * `goal_id` - ID of the goal to add funds to
    /// * `amount` - Amount to add (must be positive)
    ///
    /// # Returns
    /// The updated current amount of the goal, or -1 if the goal does not exist.
    ///
    /// # Errors
    /// Returns -1 for non-existent goals; otherwise succeeds.
    pub fn add_to_goal(env: Env, goal_id: u32, amount: i128) -> i128 {
        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        if let Some(mut goal) = goals.get(goal_id) {
            goal.current_amount += amount;
            goals.set(goal_id, goal.clone());
            env.storage()
                .instance()
                .set(&symbol_short!("GOALS"), &goals);
            goal.current_amount
        } else {
            -1 // Goal not found
        }
    }

    /// Get a savings goal by ID
    ///
    /// # Arguments
    /// * `goal_id` - ID of the goal to retrieve
    ///
    /// # Returns
    /// * `Some(SavingsGoal)` - The goal struct if found
    /// * `None` - If the goal does not exist
    ///
    /// # Errors
    /// No errors; returns None for non-existent goals.
    pub fn get_goal(env: Env, goal_id: u32) -> Option<SavingsGoal> {
        let goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        goals.get(goal_id)
    }

    /// Get all savings goals
    ///
    /// # Returns
    /// A vector of all `SavingsGoal` structs. Returns an empty vector if no goals exist.
    ///
    /// # Errors
    /// No errors; always returns a vector.
    pub fn get_all_goals(env: Env) -> Vec<SavingsGoal> {
        let goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        for i in 1..=env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32)
        {
            if let Some(goal) = goals.get(i) {
                result.push_back(goal);
            }
        }
        result
    }

    /// Check if a goal is completed
    ///
    /// # Arguments
    /// * `goal_id` - ID of the goal to check
    ///
    /// # Returns
    /// * `true` - If current_amount >= target_amount
    /// * `false` - If goal not found or not completed
    ///
    /// # Errors
    /// Returns false for non-existent goals.
    pub fn is_goal_completed(env: Env, goal_id: u32) -> bool {
        if let Some(goal) = Self::get_goal(env, goal_id) {
            goal.current_amount >= goal.target_amount
        } else {
            false
        }
    }
}
