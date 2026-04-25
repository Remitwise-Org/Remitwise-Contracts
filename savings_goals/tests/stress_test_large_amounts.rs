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

use savings_goals::{
    ContributionItem, SavingsGoalContract, SavingsGoalContractClient, SavingsGoalsError,
};
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
fn test_add_to_goal_overflow_returns_error() {
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

    // First addition should succeed
    env.mock_all_auths();
    let first = client.add_to_goal(&owner, &goal_id, &overflow_amount);
    assert_eq!(first, overflow_amount);

    // Second addition should return an overflow error rather than panic
    env.mock_all_auths();
    let result = client.try_add_to_goal(&owner, &goal_id, &overflow_amount);
    assert_eq!(result, Err(Ok(SavingsGoalsError::Overflow)));
}

#[test]
fn test_batch_add_to_goals_overflow_returns_error() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_target = i128::MAX;
    let contribution = i128::MAX / 2 + 1;

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Batch Overflow Goal"),
        &large_target,
        &2000000,
    );

    env.mock_all_auths();
    let mut contributions = Vec::new(&env);
    contributions.push_back(ContributionItem {
        goal_id,
        amount: contribution,
    });
    contributions.push_back(ContributionItem {
        goal_id,
        amount: contribution,
    });

    env.mock_all_auths();
    let result = client.try_batch_add_to_goals(&owner, &contributions);
    assert_eq!(result, Err(Ok(SavingsGoalsError::Overflow)));
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
    let to_withdraw = large_amount / 2;
    let remaining = client.withdraw_from_goal(&owner, &goal_id, &to_withdraw);

    // For odd large_amount values, large_amount - (large_amount / 2) equals
    // ceil(large_amount / 2), not exactly large_amount / 2. Assert on the
    // invariant instead of assuming evenness.
    assert_eq!(remaining + to_withdraw, large_amount);
}
// #[test]
// fn test_withdraw_from_goal_with_large_amount() {
//     let env = Env::default();
//     let contract_id = env.register_contract(None, SavingsGoalContract);
//     let client = SavingsGoalContractClient::new(&env, &contract_id);
//     let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

//     env.mock_all_auths();

//     let large_target = i128::MAX / 2;
//     let large_amount = i128::MAX / 4;

//     let goal_id = client.create_goal(
//         &owner,
//         &String::from_str(&env, "Large Goal"),
//         &large_target,
//         &2000000,
//     );

//     // Add funds
//     env.mock_all_auths();
//     client.add_to_goal(&owner, &goal_id, &large_amount);

//     // Unlock to allow withdrawal
//     env.mock_all_auths();
//     client.unlock_goal(&owner, &goal_id);

//     // Withdraw half
//     env.mock_all_auths();
//     let remaining = client.withdraw_from_goal(&owner, &goal_id, &(large_amount / 2));

//     assert_eq!(remaining, large_amount / 2);
// }
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
    let amounts_to_test = [
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

#[test]
fn test_add_to_goal_near_safe_cap_boundary() {
    // Test adding amounts right at the boundary of safe operations.
    // The contract documents max safe goal amount as i128::MAX/2 to allow
    // for safe addition operations without overflow.
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let safe_cap = i128::MAX / 2;
    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Near-Cap Goal"),
        &safe_cap,
        &2000000,
    );

    // Add amount that brings us to exactly safe_cap
    env.mock_all_auths();
    let first = client.add_to_goal(&owner, &goal_id, &safe_cap);
    assert_eq!(first, safe_cap);

    // Verify goal is now at capacity
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, safe_cap);
    assert!(goal.current_amount >= safe_cap); // At or past target
}

#[test]
fn test_add_to_goal_just_over_safe_cap_returns_overflow() {
    /// Test that adding beyond i128::MAX/2 reliably returns Overflow error
    /// rather than panicking or wrapping around.
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let safe_cap = i128::MAX / 2;
    let beyond_cap = safe_cap + 1;

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Over-Cap Goal"),
        &i128::MAX,
        &2000000,
    );

    // First add at safe boundary
    env.mock_all_auths();
    let first = client.add_to_goal(&owner, &goal_id, &safe_cap);
    assert_eq!(first, safe_cap);

    // Try to add just over boundary — must fail gracefully with Overflow
    env.mock_all_auths();
    let result = client.try_add_to_goal(&owner, &goal_id, &beyond_cap);
    assert_eq!(result, Err(Ok(SavingsGoalsError::Overflow)));
}

#[test]
fn test_withdraw_from_goal_near_underflow() {
    // Test that withdrawal near zero boundaries is handled correctly
    // and doesn't cause negative wrapping.
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let small_amount = 1000i128;
    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Small Goal"),
        &small_amount,
        &2000000,
    );

    // Add small amount
    env.mock_all_auths();
    client.add_to_goal(&owner, &goal_id, &small_amount);

    // Unlock for withdrawal
    env.mock_all_auths();
    client.unlock_goal(&owner, &goal_id);

    // Withdraw exactly what we added
    env.mock_all_auths();
    let remaining = client.withdraw_from_goal(&owner, &goal_id, &small_amount);
    assert_eq!(remaining, 0);

    // Verify goal is now empty
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 0);
}

#[test]
fn test_withdraw_from_goal_overflow_protection() {
    // Test that attempting to withdraw more than available returns error
    // instead of panicking or allowing negative amounts.
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let amount = i128::MAX / 10;
    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Withdrawal Test"),
        &(i128::MAX / 2),
        &2000000,
    );

    // Add funds
    env.mock_all_auths();
    client.add_to_goal(&owner, &goal_id, &amount);

    // Unlock
    env.mock_all_auths();
    client.unlock_goal(&owner, &goal_id);

    // Try to withdraw more than available — must fail gracefully
    env.mock_all_auths();
    let withdrawing = amount + 1;
    let result = client.try_withdraw_from_goal(&owner, &goal_id, &withdrawing);
    assert_eq!(result, Err(Ok(SavingsGoalsError::InsufficientBalance)));

    // Verify amount was not modified
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, amount);
}

#[test]
fn test_concurrent_near_boundary_operations_deterministic() {
    // Test that multiple operations near safe boundaries produce
    // deterministic results and consistent error codes.
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let safe_cap = i128::MAX / 2;
    let half_cap = safe_cap / 2;

    // Create three goals at boundary
    let goal1 = client.create_goal(
        &owner,
        &String::from_str(&env, "Goal 1"),
        &safe_cap,
        &2000000,
    );

    env.mock_all_auths();
    let goal2 = client.create_goal(
        &owner,
        &String::from_str(&env, "Goal 2"),
        &safe_cap,
        &2000000,
    );

    env.mock_all_auths();
    let goal3 = client.create_goal(
        &owner,
        &String::from_str(&env, "Goal 3"),
        &safe_cap,
        &2000000,
    );

    // Add half_cap to each
    env.mock_all_auths();
    client.add_to_goal(&owner, &goal1, &half_cap);

    env.mock_all_auths();
    client.add_to_goal(&owner, &goal2, &half_cap);

    env.mock_all_auths();
    client.add_to_goal(&owner, &goal3, &half_cap);

    // Now try to add another half_cap to each — should all succeed
    env.mock_all_auths();
    let g1_total = client.add_to_goal(&owner, &goal1, &half_cap);
    assert_eq!(g1_total, safe_cap);

    env.mock_all_auths();
    let g2_total = client.add_to_goal(&owner, &goal2, &half_cap);
    assert_eq!(g2_total, safe_cap);

    env.mock_all_auths();
    let g3_total = client.add_to_goal(&owner, &goal3, &half_cap);
    assert_eq!(g3_total, safe_cap);

    // Try one more addition to each — all should fail with same error code
    env.mock_all_auths();
    let r1 = client.try_add_to_goal(&owner, &goal1, &1);
    assert_eq!(r1, Err(Ok(SavingsGoalsError::Overflow)));

    env.mock_all_auths();
    let r2 = client.try_add_to_goal(&owner, &goal2, &1);
    assert_eq!(r2, Err(Ok(SavingsGoalsError::Overflow)));

    env.mock_all_auths();
    let r3 = client.try_add_to_goal(&owner, &goal3, &1);
    assert_eq!(r3, Err(Ok(SavingsGoalsError::Overflow)));
}

#[test]
fn test_error_codes_stable_across_repeated_operations() {
    // Test that repeated operations near boundaries consistently
    // return the same error code, proving deterministic error handling.
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Error Test"),
        &1000i128,
        &2000000,
    );

    // Fill goal
    env.mock_all_auths();
    client.add_to_goal(&owner, &goal_id, &1000);

    // Unlock
    env.mock_all_auths();
    client.unlock_goal(&owner, &goal_id);

    // Try insufficient withdrawal multiple times — all should fail identically
    for _ in 0..3 {
        env.mock_all_auths();
        let result = client.try_withdraw_from_goal(&owner, &goal_id, &2000);
        assert_eq!(
            result,
            Err(Ok(SavingsGoalsError::InsufficientBalance)),
            "Error code must be stable across repeated operations"
        );
    }

    // Verify goal amount unchanged
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 1000);
}
