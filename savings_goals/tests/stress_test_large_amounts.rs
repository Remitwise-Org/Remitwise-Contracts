#![cfg(test)]

//! Stress tests for arithmetic operations with very large i128 values in savings_goals
//!
//! These tests verify that the savings_goals contract handles extreme values correctly:
//! - Values near i128::MAX/2 to avoid overflow in additions
//! - Proper error handling for overflow conditions using checked_add/checked_sub
//! - No unexpected panics or wrap-around behavior
//!
//! ## Documented Limitations
//! - Maximum safe goal amount: i128::MAX/2 (to allow for safe addition operations)
//! - add_to_goal uses checked_add internally and will panic with "overflow" on overflow
//! - withdraw_from_goal uses checked_sub internally and will panic with "underflow" on underflow
//! - No explicit caps are imposed by the contract, but overflow/underflow will panic
//! - batch_add_to_goals has same limitations as add_to_goal for each contribution

use savings_goals::{ContributionItem, SavingsGoalContract, SavingsGoalContractClient};
use soroban_sdk::testutils::{Address as AddressTrait, Ledger, LedgerInfo};
use soroban_sdk::{Env, String, Vec};

fn set_time(env: &Env, timestamp: u64) {
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 1,
        timestamp,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100000,
    });
}
#[test]
fn test_create_goal_near_max_i128() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Test with i128::MAX / 2 - a very large but safe value
    let large_target = i128::MAX / 2;

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Large Goal"),
        &large_target,
        &2000000,
    );

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.target_amount, large_target);
    assert_eq!(goal.current_amount, 0);
}
#[test]
fn test_add_to_goal_with_large_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_target = i128::MAX / 2;
    let large_contribution = i128::MAX / 4;

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Large Goal"),
        &large_target,
        &2000000,
    );

    env.mock_all_auths();
    let new_total = client.add_to_goal(&owner, &goal_id, &large_contribution);

    assert_eq!(new_total, large_contribution);

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, large_contribution);
}
#[test]
fn test_add_to_goal_multiple_large_contributions() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_target = i128::MAX / 2;
    let contribution = i128::MAX / 10;

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Large Goal"),
        &large_target,
        &2000000,
    );

    // Add multiple times safely
    env.mock_all_auths();
    let total1 = client.add_to_goal(&owner, &goal_id, &contribution);
    assert_eq!(total1, contribution);

    env.mock_all_auths();
    let total2 = client.add_to_goal(&owner, &goal_id, &contribution);
    assert_eq!(total2, contribution + contribution);

    env.mock_all_auths();
    let total3 = client.add_to_goal(&owner, &goal_id, &contribution);
    assert_eq!(total3, contribution + contribution + contribution);
}
#[test]
#[should_panic(expected = "overflow")]
fn test_add_to_goal_overflow_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_target = i128::MAX;
    let overflow_amount = i128::MAX / 2 + 1000;

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Overflow Goal"),
        &large_target,
        &2000000,
    );

    // First addition
    env.mock_all_auths();
    client.add_to_goal(&owner, &goal_id, &overflow_amount);

    // Second addition should overflow
    env.mock_all_auths();
    client.add_to_goal(&owner, &goal_id, &overflow_amount);
}
#[test]
fn test_withdraw_from_goal_with_large_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_target = i128::MAX / 2;
    let large_amount = i128::MAX / 4;

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Large Goal"),
        &large_target,
        &2000000,
    );

    // Add funds
    env.mock_all_auths();
    client.add_to_goal(&owner, &goal_id, &large_amount);

    // Unlock to allow withdrawal
    env.mock_all_auths();
    client.unlock_goal(&owner, &goal_id);

    // Withdraw half
    env.mock_all_auths();
    let remaining = client.withdraw_from_goal(&owner, &goal_id, &(large_amount / 2));

    assert_eq!(remaining, large_amount / 2);
}
#[test]
fn test_goal_completion_with_large_amounts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_target = i128::MAX / 4;

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Large Goal"),
        &large_target,
        &2000000,
    );

    // Add exactly the target amount
    env.mock_all_auths();
    client.add_to_goal(&owner, &goal_id, &large_target);

    // Verify goal is completed
    let is_completed = client.is_goal_completed(&goal_id);
    assert!(is_completed);

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, large_target);
    assert!(goal.current_amount >= goal.target_amount);
}
#[test]
fn test_batch_add_with_large_amounts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_target = i128::MAX / 10;
    let contribution = i128::MAX / 50;

    // Create multiple goals
    let goal1 = client.create_goal(
        &owner,
        &String::from_str(&env, "Goal 1"),
        &large_target,
        &2000000,
    );

    env.mock_all_auths();
    let goal2 = client.create_goal(
        &owner,
        &String::from_str(&env, "Goal 2"),
        &large_target,
        &2000000,
    );

    env.mock_all_auths();
    let goal3 = client.create_goal(
        &owner,
        &String::from_str(&env, "Goal 3"),
        &large_target,
        &2000000,
    );

    // Batch add to all goals
    let mut contributions = Vec::new(&env);
    contributions.push_back(ContributionItem {
        goal_id: goal1,
        amount: contribution,
    });
    contributions.push_back(ContributionItem {
        goal_id: goal2,
        amount: contribution,
    });
    contributions.push_back(ContributionItem {
        goal_id: goal3,
        amount: contribution,
    });

    env.mock_all_auths();
    let count = client.batch_add_to_goals(&owner, &contributions);

    assert_eq!(count, 3);

    // Verify all goals received the contribution
    let g1 = client.get_goal(&goal1).unwrap();
    let g2 = client.get_goal(&goal2).unwrap();
    let g3 = client.get_goal(&goal3).unwrap();

    assert_eq!(g1.current_amount, contribution);
    assert_eq!(g2.current_amount, contribution);
    assert_eq!(g3.current_amount, contribution);
}
#[test]
fn test_multiple_goals_with_large_amounts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_target = i128::MAX / 10;

    // Create multiple goals with large targets
    for i in 0..5 {
        client.create_goal(
            &owner,
            &String::from_str(&env, &format!("Goal {}", i)),
            &large_target,
            &2000000,
        );
        env.mock_all_auths();
    }

    // Verify all goals were created correctly
    let goals = client.get_all_goals(&owner);
    assert_eq!(goals.len(), 5);

    for goal in goals.iter() {
        assert_eq!(goal.target_amount, large_target);
        assert_eq!(goal.current_amount, 0);
    }
}
#[test]
fn test_edge_case_i128_max_minus_one() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Test with i128::MAX - 1
    let edge_target = i128::MAX - 1;

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Edge Case"),
        &edge_target,
        &2000000,
    );

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.target_amount, edge_target);
}
#[test]
fn test_pagination_with_large_amounts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_target = i128::MAX / 100;

    // Create multiple goals with large targets
    for i in 0..15 {
        client.create_goal(
            &owner,
            &String::from_str(&env, &format!("Goal {}", i)),
            &large_target,
            &2000000,
        );
        env.mock_all_auths();
    }

    // Test pagination
    let page1 = client.get_goals(&owner, &0, &10);
    assert_eq!(page1.count, 10);
    assert!(page1.next_cursor > 0);

    let page2 = client.get_goals(&owner, &page1.next_cursor, &10);
    assert_eq!(page2.count, 5);
    assert_eq!(page2.next_cursor, 0); // No more pages

    // Verify all amounts are correct
    for goal in page1.items.iter() {
        assert_eq!(goal.target_amount, large_target);
    }
    for goal in page2.items.iter() {
        assert_eq!(goal.target_amount, large_target);
    }
}
#[test]
fn test_lock_unlock_with_large_amounts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_target = i128::MAX / 4;
    let large_amount = i128::MAX / 8;

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Large Goal"),
        &large_target,
        &2000000,
    );

    // Add funds
    env.mock_all_auths();
    client.add_to_goal(&owner, &goal_id, &large_amount);

    // Goal starts locked
    let goal = client.get_goal(&goal_id).unwrap();
    assert!(goal.locked);

    // Unlock
    env.mock_all_auths();
    client.unlock_goal(&owner, &goal_id);

    let goal = client.get_goal(&goal_id).unwrap();
    assert!(!goal.locked);

    // Lock again
    env.mock_all_auths();
    client.lock_goal(&owner, &goal_id);

    let goal = client.get_goal(&goal_id).unwrap();
    assert!(goal.locked);
}
#[test]
fn test_sequential_large_operations() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Test with progressively larger amounts
    let amounts_to_test = vec![
        i128::MAX / 1000,
        i128::MAX / 500,
        i128::MAX / 200,
        i128::MAX / 100,
        i128::MAX / 50,
    ];

    for (i, amount) in amounts_to_test.iter().enumerate() {
        let goal_id = client.create_goal(
            &owner,
            &String::from_str(&env, &format!("Goal {}", i)),
            amount,
            &2000000,
        );

        env.mock_all_auths();
        client.add_to_goal(&owner, &goal_id, &(amount / 2));

        let goal = client.get_goal(&goal_id).unwrap();
        assert_eq!(goal.current_amount, amount / 2);
        assert_eq!(goal.target_amount, *amount);

        env.mock_all_auths();
    }
}
#[test]
fn test_time_lock_with_large_amounts() {
    let env = Env::default();
    set_time(&env, 1000000);

    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_target = i128::MAX / 4;
    let large_amount = i128::MAX / 8;

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Time-locked Goal"),
        &large_target,
        &2000000,
    );

    // Add funds
    env.mock_all_auths();
    client.add_to_goal(&owner, &goal_id, &large_amount);

    // Set time lock
    env.mock_all_auths();
    client.set_time_lock(&owner, &goal_id, &2000000);

    // Unlock the goal
    env.mock_all_auths();
    client.unlock_goal(&owner, &goal_id);

    // Try to withdraw before time lock expires (should fail)
    env.mock_all_auths();
    let result = client.try_withdraw_from_goal(&owner, &goal_id, &1000);
    assert!(result.is_err());

    // Advance time past the lock
    set_time(&env, 2000001);

    // Now withdrawal should succeed
    env.mock_all_auths();
    let remaining = client.withdraw_from_goal(&owner, &goal_id, &1000);
    assert_eq!(remaining, large_amount - 1000);
}
#[test]
fn test_export_import_snapshot_with_large_amounts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_target = i128::MAX / 10;
    let large_amount = i128::MAX / 20;

    // Create goals with large amounts
    let goal1 = client.create_goal(
        &owner,
        &String::from_str(&env, "Goal 1"),
        &large_target,
        &2000000,
    );

    env.mock_all_auths();
    client.add_to_goal(&owner, &goal1, &large_amount);

    env.mock_all_auths();
    let goal2 = client.create_goal(
        &owner,
        &String::from_str(&env, "Goal 2"),
        &large_target,
        &2000000,
    );

    env.mock_all_auths();
    client.add_to_goal(&owner, &goal2, &large_amount);

    // Export snapshot
    env.mock_all_auths();
    let snapshot = client.export_snapshot(&owner);

    assert_eq!(snapshot.goals.len(), 2);
    assert_eq!(snapshot.goals.get(0).unwrap().target_amount, large_target);
    assert_eq!(snapshot.goals.get(0).unwrap().current_amount, large_amount);

    // Import snapshot (with nonce)
    env.mock_all_auths();
    let success = client.import_snapshot(&owner, &0, &snapshot);
    assert!(success);
}
