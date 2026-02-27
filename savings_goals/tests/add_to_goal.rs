use savings_goals::{SavingsGoalContract, SavingsGoalContractClient};
// The AddressTrait is necessary for .generate()
use soroban_sdk::testutils::{Address as AddressTrait, Ledger};
use soroban_sdk::{Address, Env, String};

/// Helper to set up the testing environment
fn bench_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();

    // Initializing ledger info to simulate a real network state
    env.ledger().with_mut(|info| {
        info.timestamp = 1_700_000_000;
        info.sequence_number = 1;
    });

    let mut budget = env.budget();
    budget.reset_unlimited();
    env
}

#[test]
fn test_add_to_goal_unauthorized_access() {
    let env = bench_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);

    // 1. Setup: Create User A (Owner) and User B (Attacker)
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);

    // 2. Owner A creates a goal
    let goal_name = String::from_str(&env, "Owner A Goal");
    let target_amount = 10_000i128;
    let deadline = 1_800_000u64;

    let goal_id = client.create_goal(&owner_a, &goal_name, &target_amount, &deadline);

    // 3. The "Attack": User B tries to call add_to_goal for User A's goal_id
    let deposit_amount = 500i128;

    // We use 'try_add_to_goal' to catch the panic/error result
    let result = client.try_add_to_goal(&owner_b, &goal_id, &deposit_amount);

    // 4. Assertion: Verify it failed
    assert!(
        result.is_err(),
        "Security breach: A non-owner was able to add funds to a goal!"
    );

    // 5. Assertion: Verify no funds were actually added (Rollback check)
    // We use get_all_goals or get_goal to check the state
    let goal_option = client.get_goal(&goal_id);
    let goal = goal_option.unwrap(); // This turns Option<SavingsGoal> into SavingsGoal
    assert_eq!(
        goal.current_amount, 0,
        "Balance changed despite authorization failure"
    );

    println!("Successfully verified that only the goal owner can add funds.");
}
