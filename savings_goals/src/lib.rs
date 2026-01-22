#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Env, Map, String, Symbol, Vec,
};

#[contract]
pub struct SavingsGoalContract;

#[contracttype]
#[derive(Clone)]
pub struct SavingsGoal {
    pub id: u32,
    pub name: String,
    pub target_amount: i128,
    pub current_amount: i128,
    pub target_date: u64,
    pub locked: bool,
}

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
    pub fn create_goal(env: Env, name: String, target_amount: i128, target_date: u64) -> u32 {
        assert!(target_amount > 0, "Target amount must be positive");
        assert!(
            target_date > env.ledger().timestamp(),
            "Target date must be in the future"
        );

        let storage = env.storage().persistent();

        // Load goals map or create new
        let mut goals: Map<u32, SavingsGoal> =
            storage.get(&Self::STORAGE_GOALS).unwrap_or(Map::new(&env));

        // Load next goal ID
        let next_id: u32 = storage.get(&Self::STORAGE_NEXT_ID).unwrap_or(1u32);

        let goal = SavingsGoal {
            id: next_id,
            name,
            target_amount,
            current_amount: 0,
            target_date,
            locked: true,
        };

        // Save the goal
        goals.set(next_id, goal.clone());
        storage.set(&Self::STORAGE_GOALS, &goals);
        storage.set(&Self::STORAGE_NEXT_ID, &(next_id + 1));

        next_id
    }

    /// Add funds to a savings goal
    pub fn add_to_goal(env: Env, goal_id: u32, amount: i128) -> i128 {
        assert!(amount > 0, "Amount must be positive");

        let storage = env.storage().persistent();

        let mut goals: Map<u32, SavingsGoal> =
            storage.get(&Self::STORAGE_GOALS).unwrap_or(Map::new(&env));

        if let Some(mut goal) = goals.get(goal_id) {
            goal.current_amount = goal
                .current_amount
                .checked_add(amount)
                .expect("Overflow adding amount");
            goals.set(goal_id, goal.clone());
            storage.set(&Self::STORAGE_GOALS, &goals);
            goal.current_amount
        } else {
            -1
        }
    }

    /// Get a specific savings goal
    pub fn get_goal(env: Env, goal_id: u32) -> Option<SavingsGoal> {
        let storage = env.storage().persistent();
        let goals: Map<u32, SavingsGoal> =
            storage.get(&Self::STORAGE_GOALS).unwrap_or(Map::new(&env));
        goals.get(goal_id)
    }

    /// Get all savings goals
    pub fn get_all_goals(env: Env) -> Vec<SavingsGoal> {
        let storage = env.storage().persistent();
        let goals: Map<u32, SavingsGoal> =
            storage.get(&Self::STORAGE_GOALS).unwrap_or(Map::new(&env));

        let mut vec = Vec::new(&env);
        for (_, goal) in goals.iter() {
            vec.push_back(goal);
        }
        vec
    }

    /// Check if a goal is completed
    pub fn is_goal_completed(env: Env, goal_id: u32) -> bool {
        let storage = env.storage().persistent();
        let goals: Map<u32, SavingsGoal> =
            storage.get(&Self::STORAGE_GOALS).unwrap_or(Map::new(&env));

        if let Some(goal) = goals.get(goal_id) {
            goal.current_amount >= goal.target_amount
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test;
