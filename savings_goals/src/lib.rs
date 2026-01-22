#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Env, Map, String, Symbol, Vec};

// Event topics
const GOAL_CREATED: Symbol = symbol_short!("created");
const FUNDS_ADDED: Symbol = symbol_short!("added");
const GOAL_COMPLETED: Symbol = symbol_short!("completed");

// Event data structures
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
    /// * `target_amount` - Target amount to save
    /// * `target_date` - Target date as Unix timestamp
    ///
    /// # Returns
    /// The ID of the created goal
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

        // Emit GoalCreated event
        let event = GoalCreatedEvent {
            goal_id: next_id,
            name: name.clone(),
            target_amount,
            target_date,
            timestamp: env.ledger().timestamp(),
        };
        env.events().publish((GOAL_CREATED,), event);

        next_id
    }

    /// Add funds to a savings goal
    ///
    /// # Arguments
    /// * `goal_id` - ID of the goal
    /// * `amount` - Amount to add
    ///
    /// # Returns
    /// Updated current amount
    pub fn add_to_goal(env: Env, goal_id: u32, amount: i128) -> i128 {
        let mut goals: Map<u32, SavingsGoal> = env
            .storage()
            .instance()
            .get(&symbol_short!("GOALS"))
            .unwrap_or_else(|| Map::new(&env));

        if let Some(mut goal) = goals.get(goal_id) {
            goal.current_amount += amount;
            let new_total = goal.current_amount;
            let was_completed = goal.current_amount >= goal.target_amount;

            goals.set(goal_id, goal.clone());
            env.storage()
                .instance()
                .set(&symbol_short!("GOALS"), &goals);

            // Emit FundsAdded event
            let funds_event = FundsAddedEvent {
                goal_id,
                amount,
                new_total,
                timestamp: env.ledger().timestamp(),
            };
            env.events().publish((FUNDS_ADDED,), funds_event);

            // Emit GoalCompleted event if goal just reached target
            if was_completed && (new_total - amount) < goal.target_amount {
                let completed_event = GoalCompletedEvent {
                    goal_id,
                    name: goal.name.clone(),
                    final_amount: new_total,
                    timestamp: env.ledger().timestamp(),
                };
                env.events().publish((GOAL_COMPLETED,), completed_event);
            }

            goal.current_amount
        } else {
            -1 // Goal not found
        }
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

    /// Get all savings goals
    ///
    /// # Returns
    /// Vec of all SavingsGoal structs
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
    /// * `goal_id` - ID of the goal
    ///
    /// # Returns
    /// True if current_amount >= target_amount
    pub fn is_goal_completed(env: Env, goal_id: u32) -> bool {
        if let Some(goal) = Self::get_goal(env, goal_id) {
            goal.current_amount >= goal.target_amount
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Events;

    #[test]
    fn test_create_goal_emits_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SavingsGoals);
        let client = SavingsGoalsClient::new(&env, &contract_id);

        // Create a goal
        let goal_id = client.create_goal(
            &String::from_str(&env, "Education"),
            &10000,
            &1735689600, // Future date
        );
        assert_eq!(goal_id, 1);

        // Verify event was emitted
        let events = env.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_add_to_goal_emits_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SavingsGoals);
        let client = SavingsGoalsClient::new(&env, &contract_id);

        // Create a goal
        let goal_id = client.create_goal(
            &String::from_str(&env, "Medical"),
            &5000,
            &1735689600,
        );

        // Get events before adding funds
        let events_before = env.events().all().len();

        // Add funds
        let new_amount = client.add_to_goal(&goal_id, &1000);
        assert_eq!(new_amount, 1000);

        // Verify 1 new event was emitted (FundsAdded event)
        let events_after = env.events().all().len();
        assert_eq!(events_after - events_before, 1);
    }

    #[test]
    fn test_goal_completed_emits_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SavingsGoals);
        let client = SavingsGoalsClient::new(&env, &contract_id);

        // Create a goal with small target
        let goal_id = client.create_goal(
            &String::from_str(&env, "Emergency Fund"),
            &1000,
            &1735689600,
        );

        // Get events before adding funds
        let events_before = env.events().all().len();

        // Add funds to complete the goal
        client.add_to_goal(&goal_id, &1000);

        // Verify both FundsAdded and GoalCompleted events were emitted (2 new events)
        let events_after = env.events().all().len();
        assert_eq!(events_after - events_before, 2);
    }

    #[test]
    fn test_multiple_goals_emit_separate_events() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SavingsGoals);
        let client = SavingsGoalsClient::new(&env, &contract_id);

        // Create multiple goals
        client.create_goal(&String::from_str(&env, "Goal 1"), &1000, &1735689600);
        client.create_goal(&String::from_str(&env, "Goal 2"), &2000, &1735689600);
        client.create_goal(&String::from_str(&env, "Goal 3"), &3000, &1735689600);

        // Should have 3 GoalCreated events
        let events = env.events().all();
        assert_eq!(events.len(), 3);
    }
}
