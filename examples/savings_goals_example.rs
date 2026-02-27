use soroban_sdk::{Env, Address, String, testutils::Address as _};
use savings_goals::{SavingsGoalContract, SavingsGoalContractClient};

fn main() {
    // 1. Setup the Soroban environment
    let env = Env::default();
    env.mock_all_auths();

    // 2. Register the SavingsGoals contract
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);

    // 3. Generate a mock owner address
    let owner = Address::generate(&env);

    println!("--- Remitwise: Savings Goals Example ---");

    // 4. [Write] Create a new savings goal
    let goal_name = String::from_str(&env, "Emergency Fund");
    let target_amount = 5000i128;
    let target_date = env.ledger().timestamp() + 31536000; // 1 year from now

    println!("Creating savings goal: '{}' with target: {}", goal_name, target_amount);
    let goal_id = client.create_goal(&owner, &goal_name, &target_amount, &target_date).unwrap();
    println!("Goal created successfully with ID: {}", goal_id);

    // 5. [Read] Fetch the goal to check progress
    let goal = client.get_goal(&goal_id).unwrap();
    println!("\nGoal Details:");
    println!("  Name: {}", goal.name);
    println!("  Current Amount: {}", goal.current_amount);
    println!("  Target Amount: {}", goal.target_amount);
    println!("  Locked: {}", goal.locked);

    // 6. [Write] Add funds to the goal
    let contribution = 1000i128;
    println!("\nContributing {} to the goal...", contribution);
    let new_total = client.add_to_goal(&owner, &goal_id, &contribution).unwrap();
    println!("Contribution successful! New total: {}", new_total);

    // 7. [Read] Verify progress again
    let updated_goal = client.get_goal(&goal_id).unwrap();
    println!("Updated Current Amount: {}", updated_goal.current_amount);

    println!("\nExample completed successfully!");
}
