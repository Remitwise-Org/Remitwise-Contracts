#![no_std]
//! # Savings Goals Contract
//!
//! This contract enables users to create and track savings goals with target amounts
//! and dates. Goals can be locked to prevent withdrawals before target date.
//!
//! ## Features
//! - Create savings goals with target amounts and dates
//! - Add funds to goals incrementally
//! - Track progress toward goals
//! - Check goal completion status

use soroban_sdk::{
    contract, contractimpl, symbol_short, vec, Env, Map, Symbol, Vec, String,
};

/// Represents a savings goal with target and progress tracking
///
/// # Fields
/// * `id` - Unique identifier for the goal
/// * `name` - Name of the goal (e.g., "Education", "Medical Emergency")
/// * `target_amount` - Target amount to save in stroops
/// * `current_amount` - Currently saved amount in stroops
/// * `target_date` - Unix timestamp of the target completion date
/// * `locked` - Whether funds are locked until target date
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

/// Smart contract for managing personal savings goals
///
/// This contract allows users to define and track savings goals with
/// target amounts and completion dates, helping them build financial discipline.
#[contractimpl]
impl SavingsGoals {
    /// Create a new savings goal
    /// 
    /// Creates a new savings goal with a target amount and date.
    /// The goal starts with current_amount = 0 and locked = true.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `name` - Name of the goal (e.g., "Education", "Medical Emergency")
    /// * `target_amount` - Target amount to save in stroops
    /// * `target_date` - Target completion date as Unix timestamp
    /// 
    /// # Returns
    /// The ID of the created goal as u32
    ///
    /// # Example
    /// ```ignore
    /// let goal_id = SavingsGoals::create_goal(
    ///     env,
    ///     String::from_small_str("Education"),
    ///     1_000_000_000, // 100 USDC
    ///     1735689600    // 2025-01-01
    /// );
    /// ```
    pub fn create_goal(
        env: Env,
        name: String,
        target_amount: i128,
        target_date: u64,
    ) -> u32 {
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
        env.storage().instance().set(&symbol_short!("GOALS"), &goals);
        env.storage().instance().set(&symbol_short!("NEXT_ID"), &next_id);
        
        next_id
    }
    
    /// Add funds to a savings goal
    /// 
    /// Deposits funds into a specific savings goal, incrementing the current_amount.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `goal_id` - ID of the goal
    /// * `amount` - Amount to add in stroops
    /// 
    /// # Returns
    /// Updated current amount, or -1 if goal not found
    ///
    /// # Error Codes
    /// - Goal not found: returns -1
    pub fn add_to_goal(env: Env, goal_id: u32, amount: i128) -> i128 {
        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));
        
        if let Some(mut goal) = goals.get(goal_id) {
            goal.current_amount += amount;
            goals.set(goal_id, goal.clone());
            env.storage().instance().set(&symbol_short!("GOALS"), &goals);
            goal.current_amount
        } else {
            -1 // Goal not found
        }
    }
    
    /// Get a savings goal by ID
    /// 
    /// Retrieves a specific savings goal from storage.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `goal_id` - ID of the goal
    /// 
    /// # Returns
    /// Option<SavingsGoal> - Some(goal) if found, None otherwise
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
    /// Retrieves all savings goals in the contract.
    ///
    /// # Returns
    /// Vec<SavingsGoal> - Vector of all goals
    pub fn get_all_goals(env: Env) -> Vec<SavingsGoal> {
        let goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));
        
        let mut result = Vec::new(&env);
        for i in 1..=env.storage().instance().get(&symbol_short!("NEXT_ID")).unwrap_or(0u32) {
            if let Some(goal) = goals.get(i) {
                result.push_back(goal);
            }
        }
        result
    }
    
    /// Check if a goal is completed
    /// 
    /// Determines if the current saved amount meets or exceeds the target amount.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `goal_id` - ID of the goal
    /// 
    /// # Returns
    /// True if current_amount >= target_amount, false if goal not found or not completed
    pub fn is_goal_completed(env: Env, goal_id: u32) -> bool {
        if let Some(goal) = Self::get_goal(env, goal_id) {
            goal.current_amount >= goal.target_amount
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test;

