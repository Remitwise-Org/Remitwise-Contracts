#![cfg(test)]

use super::*;
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::{
    testutils::{Address as AddressTrait, Events, Ledger, LedgerInfo},
    Address, Env, String, Symbol, TryFromVal,
};

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
fn test_create_goal_unique_ids() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();

    let name1 = String::from_str(&env, "Goal 1");
    let name2 = String::from_str(&env, "Goal 2");

    // Tell the environment to auto-approve the 'user' signature
    env.mock_all_auths();

    let id1 = client.create_goal(&user, &name1, &1000, &1735689600);
    let id2 = client.create_goal(&user, &name2, &2000, &1735689600);

    assert_ne!(id1, id2);
}

// ============================================================================
// init() idempotency and NEXT_ID behavior
//
// init() bootstraps storage (NEXT_ID and GOALS) only when keys are missing.
// In production or integration, init() may be called more than once (e.g. by
// different entrypoints or upgrade paths). These tests lock in that:
// - A second init() must not remove or alter existing goals.
// - NEXT_ID must not be reset by a second init(); the next created goal must
//   receive the expected incremented ID (no reuse, no gaps).
// ============================================================================

/// Double init() must not remove or alter existing goals; next created goal
/// must get the next ID (e.g. 2), not 1.
#[test]
fn test_init_idempotent_does_not_wipe_goals() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner_a = Address::generate(&env);

    // First init on a fresh contract
    client.init();

    let name1 = String::from_str(&env, "First Goal");
    let target1 = 5000i128;
    let target_date1 = 2000000000u64;

    let goal_id_1 = client.create_goal(&owner_a, &name1, &target1, &target_date1);
    assert_eq!(goal_id_1, 1, "first goal must receive goal_id == 1");

    // Simulate a second initialization attempt (e.g. from another entrypoint or upgrade)
    client.init();

    // Verify the existing goal is still present with same name, owner, amounts
    let goal_after_second_init = client
        .get_goal(&1)
        .expect("goal 1 must still exist after second init()");
    assert_eq!(goal_after_second_init.name, name1);
    assert_eq!(goal_after_second_init.owner, owner_a);
    assert_eq!(goal_after_second_init.target_amount, target1);
    assert_eq!(goal_after_second_init.current_amount, 0);

    let all_goals = client.get_all_goals(&owner_a);
    assert_eq!(all_goals.len(), 1, "get_all_goals must still return the one goal");

    // Verify NEXT_ID was not reset: next created goal must get goal_id == 2, not 1
    let name2 = String::from_str(&env, "Second Goal");
    let goal_id_2 = client.create_goal(&owner_a, &name2, &10000i128, &target_date1);
    assert_eq!(
        goal_id_2, 2,
        "after second init(), next goal must get goal_id == 2, not 1 (NEXT_ID must not be reset)"
    );
}

/// After init(), creating goals sequentially must yield IDs 1, 2, 3, ... with
/// no gaps or reuse.
#[test]
fn test_next_id_increments_sequentially() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();

    let ids = [
        client.create_goal(
            &owner,
            &String::from_str(&env, "G1"),
            &1000i128,
            &2000000000u64,
        ),
        client.create_goal(
            &owner,
            &String::from_str(&env, "G2"),
            &2000i128,
            &2000000000u64,
        ),
        client.create_goal(
            &owner,
            &String::from_str(&env, "G3"),
            &3000i128,
            &2000000000u64,
        ),
    ];

    assert_eq!(ids[0], 1, "first goal id must be 1");
    assert_eq!(ids[1], 2, "second goal id must be 2");
    assert_eq!(ids[2], 3, "third goal id must be 3");

    for (i, &id) in ids.iter().enumerate() {
        let goal = client.get_goal(&id).unwrap();
        assert_eq!(goal.id, id);
        let expected_name = String::from_str(&env, &format!("G{}", i + 1));
        assert_eq!(goal.name, expected_name);
    }
}

#[test]
fn test_add_to_goal_increments() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();

    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Save"), &1000, &2000000000);

    let new_balance = client.add_to_goal(&user, &id, &500);
    assert_eq!(new_balance, 500);
}

#[test]
fn test_add_to_non_existent_goal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let res = client.try_add_to_goal(&user, &99, &500);
    assert_eq!(res, Err(Ok(SavingsGoalError::GoalNotFound)));
}

#[test]
fn test_get_goal_retrieval() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let name = String::from_str(&env, "Car");
    let id = client.create_goal(&user, &name, &5000, &2000000000);

    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.name, name);
}

#[test]
fn test_get_all_goals() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    client.create_goal(&user, &String::from_str(&env, "A"), &100, &2000000000);
    client.create_goal(&user, &String::from_str(&env, "B"), &200, &2000000000);

    let all_goals = client.get_all_goals(&user);
    assert_eq!(all_goals.len(), 2);
}

#[test]
fn test_is_goal_completed() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    // 1. Create a goal with a target of 1000
    let target = 1000;
    let name = String::from_str(&env, "Trip");
    let id = client.create_goal(&user, &name, &target, &2000000000);

    // 2. It should NOT be completed initially (balance is 0)
    assert!(
        !client.is_goal_completed(&id),
        "Goal should not be complete at start"
    );

    // 3. Add exactly the target amount
    client.add_to_goal(&user, &id, &target);

    // 4. Verify the balance actually updated in storage
    let goal = client.get_goal(&id).unwrap();
    assert_eq!(
        goal.current_amount, target,
        "The amount was not saved correctly"
    );

    // 5. This will now pass once you fix the .instance() vs .persistent() mismatch in lib.rs
    assert!(
        client.is_goal_completed(&id),
        "Goal should be completed when current == target"
    );

    // 6. Bonus: Check that it stays completed if we go over the target
    client.add_to_goal(&user, &id, &1);
    assert!(
        client.is_goal_completed(&id),
        "Goal should stay completed if overfunded"
    );
}

#[test]
fn test_edge_cases_large_amounts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Max"),
        &i128::MAX,
        &2000000000,
    );

    client.add_to_goal(&user, &id, &(i128::MAX - 100));
    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.current_amount, i128::MAX - 100);
}

#[test]
fn test_zero_amount_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let res = client.try_create_goal(&user, &String::from_str(&env, "Fail"), &0, &2000000000);
    assert_eq!(res, Err(Ok(SavingsGoalError::TargetAmountMustBePositive)));
}

#[test]
fn test_multiple_goals_management() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id1 = client.create_goal(&user, &String::from_str(&env, "G1"), &1000, &2000000000);
    let id2 = client.create_goal(&user, &String::from_str(&env, "G2"), &2000, &2000000000);

    client.add_to_goal(&user, &id1, &500);
    client.add_to_goal(&user, &id2, &1500);

    let g1 = client.get_goal(&id1).unwrap();
    let g2 = client.get_goal(&id2).unwrap();

    assert_eq!(g1.current_amount, 500);
    assert_eq!(g2.current_amount, 1500);
}

#[test]
fn test_withdraw_from_goal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "W"), &1000, &2000000000);

    // Unlock first (created locked)
    client.unlock_goal(&user, &id);

    client.add_to_goal(&user, &id, &500);

    let new_balance = client.withdraw_from_goal(&user, &id, &200);
    assert_eq!(new_balance, 300);

    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.current_amount, 300);
}

#[test]
fn test_withdraw_too_much() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "W"), &1000, &2000000000);

    client.unlock_goal(&user, &id);
    client.add_to_goal(&user, &id, &100);

    let res = client.try_withdraw_from_goal(&user, &id, &200);
    assert_eq!(res, Err(Ok(SavingsGoalError::InsufficientBalance)));
}

#[test]
fn test_withdraw_locked() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "L"), &1000, &2000000000);

    // Goal is locked by default
    client.add_to_goal(&user, &id, &500);
    let res = client.try_withdraw_from_goal(&user, &id, &100);
    assert_eq!(res, Err(Ok(SavingsGoalError::GoalLocked)));
}

#[test]
fn test_withdraw_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Auth"), &1000, &2000000000);

    client.unlock_goal(&user, &id);
    client.add_to_goal(&user, &id, &500);

    let res = client.try_withdraw_from_goal(&other, &id, &100);
    assert_eq!(res, Err(Ok(SavingsGoalError::Unauthorized)));
}

#[test]
fn test_lock_unlock_goal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Lock"), &1000, &2000000000);

    let goal = client.get_goal(&id).unwrap();
    assert!(goal.locked);

    client.unlock_goal(&user, &id);
    let goal = client.get_goal(&id).unwrap();
    assert!(!goal.locked);

    client.lock_goal(&user, &id);
    let goal = client.get_goal(&id).unwrap();
    assert!(goal.locked);
}

#[test]
fn test_full_withdrawal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "W"), &1000, &2000000000);

    client.unlock_goal(&user, &id);
    client.add_to_goal(&user, &id, &500);

    // Withdraw everything
    let new_balance = client.withdraw_from_goal(&user, &id, &500);
    assert_eq!(new_balance, 0);

    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.current_amount, 0);
    assert!(!client.is_goal_completed(&id));
}

#[test]
fn test_exact_goal_completion() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Exact"), &1000, &2000000000);

    // Add 500 twice
    client.add_to_goal(&user, &id, &500);
    assert!(!client.is_goal_completed(&id));

    client.add_to_goal(&user, &id, &500);
    assert!(client.is_goal_completed(&id));

    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.current_amount, 1000);
}

#[test]
fn test_set_time_lock() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &10000, &5000);

    client.set_time_lock(&owner, &goal_id, &10000);

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.unlock_date, Some(10000));
}

#[test]
fn test_withdraw_time_locked_goal_before_unlock() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &10000, &5000);

    client.add_to_goal(&owner, &goal_id, &5000);
    client.unlock_goal(&owner, &goal_id);
    client.set_time_lock(&owner, &goal_id, &10000);

    let result = client.try_withdraw_from_goal(&owner, &goal_id, &1000);
    assert!(result.is_err());
}

#[test]
fn test_withdraw_time_locked_goal_after_unlock() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &10000, &5000);

    client.add_to_goal(&owner, &goal_id, &5000);
    client.unlock_goal(&owner, &goal_id);
    client.set_time_lock(&owner, &goal_id, &3000);

    set_time(&env, 3500);
    let new_amount = client.withdraw_from_goal(&owner, &goal_id, &1000);
    assert_eq!(new_amount, 4000);
}

#[test]
fn test_create_savings_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &10000, &5000);

    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &3000, &86400);
    assert_eq!(schedule_id, 1);

    let schedule = client.get_savings_schedule(&schedule_id);
    assert!(schedule.is_some());
    let schedule = schedule.unwrap();
    assert_eq!(schedule.amount, 500);
    assert_eq!(schedule.next_due, 3000);
    assert!(schedule.active);
}

#[test]
fn test_modify_savings_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &10000, &5000);

    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &3000, &86400);
    client.modify_savings_schedule(&owner, &schedule_id, &1000, &4000, &172800);

    let schedule = client.get_savings_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.amount, 1000);
    assert_eq!(schedule.next_due, 4000);
    assert_eq!(schedule.interval, 172800);
}

#[test]
fn test_cancel_savings_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &10000, &5000);

    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &3000, &86400);
    client.cancel_savings_schedule(&owner, &schedule_id);

    let schedule = client.get_savings_schedule(&schedule_id).unwrap();
    assert!(!schedule.active);
}

#[test]
fn test_execute_due_savings_schedules() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &10000, &5000);

    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &3000, &0);

    set_time(&env, 3500);
    let executed = client.execute_due_savings_schedules();

    assert_eq!(executed.len(), 1);
    assert_eq!(executed.get(0).unwrap(), schedule_id);

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 500);
}

#[test]
fn test_execute_recurring_savings_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &10000, &5000);

    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &3000, &86400);

    set_time(&env, 3500);
    client.execute_due_savings_schedules();

    let schedule = client.get_savings_schedule(&schedule_id).unwrap();
    assert!(schedule.active);
    assert_eq!(schedule.next_due, 3000 + 86400);

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 500);
}

#[test]
fn test_execute_missed_savings_schedules() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &10000, &5000);

    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &3000, &86400);

    set_time(&env, 3000 + 86400 * 3 + 100);
    client.execute_due_savings_schedules();

    let schedule = client.get_savings_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.missed_count, 3);
    assert!(schedule.next_due > 3000 + 86400 * 3);
}

#[test]
fn test_savings_schedule_goal_completion() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &1000, &5000);

    client.create_savings_schedule(&owner, &goal_id, &1000, &3000, &0);

    set_time(&env, 3500);
    client.execute_due_savings_schedules();

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 1000);
    assert!(client.is_goal_completed(&goal_id));
}

#[test]
fn test_lock_goal_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Lock Test"),
        &1000,
        &2000000000,
    );

    client.unlock_goal(&user, &id);
    assert!(!client.get_goal(&id).unwrap().locked);

    client.lock_goal(&user, &id);
    assert!(client.get_goal(&id).unwrap().locked);
}

#[test]
fn test_unlock_goal_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Unlock Test"),
        &1000,
        &2000000000,
    );

    assert!(client.get_goal(&id).unwrap().locked);

    client.unlock_goal(&user, &id);
    assert!(!client.get_goal(&id).unwrap().locked);
}

#[test]
fn test_lock_goal_unauthorized_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Auth Test"),
        &1000,
        &2000000000,
    );

    client.unlock_goal(&user, &id);

    let res = client.try_lock_goal(&other, &id);
    assert_eq!(res, Err(Ok(SavingsGoalError::Unauthorized)));
}

#[test]
fn test_unlock_goal_unauthorized_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Auth Test"),
        &1000,
        &2000000000,
    );

    let res = client.try_unlock_goal(&other, &id);
    assert_eq!(res, Err(Ok(SavingsGoalError::Unauthorized)));
}

#[test]
fn test_withdraw_after_lock_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Withdraw Fail"),
        &1000,
        &2000000000,
    );

    client.unlock_goal(&user, &id);
    client.add_to_goal(&user, &id, &500);
    client.lock_goal(&user, &id);

    let res = client.try_withdraw_from_goal(&user, &id, &100);
    assert_eq!(res, Err(Ok(SavingsGoalError::GoalLocked)));
}

#[test]
fn test_withdraw_after_unlock_succeeds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Withdraw Success"),
        &1000,
        &2000000000,
    );

    client.unlock_goal(&user, &id);
    client.add_to_goal(&user, &id, &500);

    let new_balance = client.withdraw_from_goal(&user, &id, &200);
    assert_eq!(new_balance, 300);

    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.current_amount, 300);
}

#[test]
fn test_lock_nonexistent_goal_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    let res = client.try_lock_goal(&user, &99);
    assert_eq!(res, Err(Ok(SavingsGoalError::GoalNotFound)));
}

#[test]
fn test_create_goal_emits_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    // Create a goal
    let goal_id = client.create_goal(
        &user,
        &String::from_str(&env, "Education"),
        &10000,
        &1735689600, // Future date
    );
    assert_eq!(goal_id, 1);

    let events = env.events().all();
    let mut found_created_struct = false;
    let mut found_created_enum = false;

    for event in events.iter() {
        let topics = event.1;
        let topic0: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();

        if topic0 == GOAL_CREATED {
            let event_data: GoalCreatedEvent =
                GoalCreatedEvent::try_from_val(&env, &event.2).unwrap();
            assert_eq!(event_data.goal_id, goal_id);
            found_created_struct = true;
        }

        if topic0 == symbol_short!("savings") && topics.len() > 1 {
            let topic1: SavingsEvent =
                SavingsEvent::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
            if matches!(topic1, SavingsEvent::GoalCreated) {
                found_created_enum = true;
            }
        }
    }

    assert!(
        found_created_struct,
        "GoalCreated struct event was not emitted"
    );
    assert!(
        found_created_enum,
        "SavingsEvent::GoalCreated was not emitted"
    );
}

#[test]
fn test_add_to_goal_emits_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    // Create a goal
    let goal_id = client.create_goal(
        &user,
        &String::from_str(&env, "Medical"),
        &5000,
        &1735689600,
    );

    // Add funds
    let new_amount = client.add_to_goal(&user, &goal_id, &1000);
    assert_eq!(new_amount, 1000);

    let events = env.events().all();
    let mut found_added_struct = false;
    let mut found_added_enum = false;

    for event in events.iter() {
        let topics = event.1;
        let topic0: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();

        if topic0 == FUNDS_ADDED {
            let event_data: FundsAddedEvent =
                FundsAddedEvent::try_from_val(&env, &event.2).unwrap();
            assert_eq!(event_data.goal_id, goal_id);
            assert_eq!(event_data.amount, 1000);
            found_added_struct = true;
        }

        if topic0 == symbol_short!("savings") && topics.len() > 1 {
            let topic1: SavingsEvent =
                SavingsEvent::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
            if matches!(topic1, SavingsEvent::FundsAdded) {
                found_added_enum = true;
            }
        }
    }

    assert!(
        found_added_struct,
        "FundsAdded struct event was not emitted"
    );
    assert!(found_added_enum, "SavingsEvent::FundsAdded was not emitted");
}

#[test]
fn test_goal_completed_emits_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    // Create a goal with small target
    let goal_id = client.create_goal(
        &user,
        &String::from_str(&env, "Emergency Fund"),
        &1000,
        &1735689600,
    );

    // Add funds to complete the goal
    client.add_to_goal(&user, &goal_id, &1000);

    let events = env.events().all();
    let mut found_completed_struct = false;
    let mut found_completed_enum = false;

    for event in events.iter() {
        let topics = event.1;
        let topic0: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();

        if topic0 == GOAL_COMPLETED {
            let event_data: GoalCompletedEvent =
                GoalCompletedEvent::try_from_val(&env, &event.2).unwrap();
            assert_eq!(event_data.goal_id, goal_id);
            assert_eq!(event_data.final_amount, 1000);
            found_completed_struct = true;
        }

        if topic0 == symbol_short!("savings") && topics.len() > 1 {
            let topic1: SavingsEvent =
                SavingsEvent::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
            if matches!(topic1, SavingsEvent::GoalCompleted) {
                found_completed_enum = true;
            }
        }
    }

    assert!(
        found_completed_struct,
        "GoalCompleted struct event was not emitted"
    );
    assert!(
        found_completed_enum,
        "SavingsEvent::GoalCompleted was not emitted"
    );
}

#[test]
fn test_withdraw_from_goal_emits_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    let goal_id = client.create_goal(
        &user,
        &String::from_str(&env, "Withdraw Event"),
        &5000,
        &1735689600,
    );
    client.unlock_goal(&user, &goal_id);
    client.add_to_goal(&user, &goal_id, &1500);
    client.withdraw_from_goal(&user, &goal_id, &600);

    let events = env.events().all();
    let mut found_withdrawn_enum = false;

    for event in events.iter() {
        let topics = event.1;
        let topic0: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        if topic0 == symbol_short!("savings") && topics.len() > 1 {
            let topic1: SavingsEvent =
                SavingsEvent::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
            if matches!(topic1, SavingsEvent::FundsWithdrawn) {
                found_withdrawn_enum = true;
            }
        }
    }

    assert!(
        found_withdrawn_enum,
        "SavingsEvent::FundsWithdrawn was not emitted"
    );
}

#[test]
fn test_lock_goal_emits_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    let goal_id = client.create_goal(
        &user,
        &String::from_str(&env, "Lock Event"),
        &5000,
        &1735689600,
    );
    client.unlock_goal(&user, &goal_id);
    client.lock_goal(&user, &goal_id);

    let events = env.events().all();
    let mut found_locked_enum = false;

    for event in events.iter() {
        let topics = event.1;
        let topic0: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        if topic0 == symbol_short!("savings") && topics.len() > 1 {
            let topic1: SavingsEvent =
                SavingsEvent::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
            if matches!(topic1, SavingsEvent::GoalLocked) {
                found_locked_enum = true;
            }
        }
    }

    assert!(
        found_locked_enum,
        "SavingsEvent::GoalLocked was not emitted"
    );
}

#[test]
fn test_unlock_goal_emits_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    let goal_id = client.create_goal(
        &user,
        &String::from_str(&env, "Unlock Event"),
        &5000,
        &1735689600,
    );
    client.unlock_goal(&user, &goal_id);

    let events = env.events().all();
    let mut found_unlocked_enum = false;

    for event in events.iter() {
        let topics = event.1;
        let topic0: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        if topic0 == symbol_short!("savings") && topics.len() > 1 {
            let topic1: SavingsEvent =
                SavingsEvent::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
            if matches!(topic1, SavingsEvent::GoalUnlocked) {
                found_unlocked_enum = true;
            }
        }
    }

    assert!(
        found_unlocked_enum,
        "SavingsEvent::GoalUnlocked was not emitted"
    );
}

#[test]
fn test_multiple_goals_emit_separate_events() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    // Create multiple goals
    client.create_goal(&user, &String::from_str(&env, "Goal 1"), &1000, &1735689600);
    client.create_goal(&user, &String::from_str(&env, "Goal 2"), &2000, &1735689600);
    client.create_goal(&user, &String::from_str(&env, "Goal 3"), &3000, &1735689600);

    // Should have 3 * 2 events = 6 events
    let events = env.events().all();
    assert_eq!(events.len(), 6);
}

// ============================================================================
// Storage TTL Extension Tests
//
// Verify that instance storage TTL is properly extended on state-changing
// operations, preventing unexpected data expiration.
//
// Contract TTL configuration:
//   INSTANCE_LIFETIME_THRESHOLD = 17,280 ledgers (~1 day)
//   INSTANCE_BUMP_AMOUNT        = 518,400 ledgers (~30 days)
//
// Operations extending instance TTL:
//   create_goal, add_to_goal, batch_add_to_goals, withdraw_from_goal,
//   lock_goal, unlock_goal, import_snapshot, set_time_lock,
//   create_savings_schedule, modify_savings_schedule,
//   cancel_savings_schedule, execute_due_savings_schedules
// ============================================================================

/// Verify that create_goal extends instance storage TTL.
#[test]
fn test_instance_ttl_extended_on_create_goal() {
    let env = Env::default();
    env.mock_all_auths();

    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 100,
        timestamp: 1000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();

    // create_goal calls extend_instance_ttl
    let goal_id = client.create_goal(
        &user,
        &String::from_str(&env, "Emergency Fund"),
        &10000,
        &1735689600,
    );
    assert!(goal_id > 0);

    // Inspect instance TTL — must be at least INSTANCE_BUMP_AMOUNT
    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= INSTANCE_BUMP_AMOUNT (518,400) after create_goal",
        ttl
    );
}

/// Verify that add_to_goal refreshes instance TTL after ledger advancement.
///
/// extend_ttl(threshold, extend_to) only extends when TTL <= threshold.
/// We advance the ledger far enough for TTL to drop below 17,280.
#[test]
fn test_instance_ttl_refreshed_on_add_to_goal() {
    let env = Env::default();
    env.mock_all_auths();

    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 100,
        timestamp: 1000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();

    let goal_id = client.create_goal(
        &user,
        &String::from_str(&env, "Vacation"),
        &5000,
        &2000000000,
    );

    // Advance ledger so TTL drops below threshold (17,280)
    // After create_goal: live_until = 518,500. At seq 510,000: TTL = 8,500
    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 510_000,
        timestamp: 500_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    // add_to_goal calls extend_instance_ttl → re-extends TTL to 518,400
    let new_balance = client.add_to_goal(&user, &goal_id, &500);
    assert_eq!(new_balance, 500);

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= 518,400 after add_to_goal",
        ttl
    );
}

/// Verify data persists across repeated operations spanning multiple
/// ledger advancements, proving TTL is continuously renewed.
#[test]
fn test_savings_data_persists_across_ledger_advancements() {
    let env = Env::default();
    env.mock_all_auths();

    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 100,
        timestamp: 1000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();

    // Phase 1: Create goals at seq 100. live_until = 518,500
    let id1 = client.create_goal(
        &user,
        &String::from_str(&env, "Education"),
        &10000,
        &2000000000,
    );
    let id2 = client.create_goal(&user, &String::from_str(&env, "House"), &50000, &2000000000);

    // Phase 2: Advance to seq 510,000 (TTL = 8,500 < 17,280)
    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 510_000,
        timestamp: 510_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    client.add_to_goal(&user, &id1, &3000);

    // Phase 3: Advance to seq 1,020,000 (TTL = 8,400 < 17,280)
    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 1_020_000,
        timestamp: 1_020_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    // Add more funds to second goal
    client.add_to_goal(&user, &id2, &10000);

    // All goals should be accessible with correct data
    let goal1 = client.get_goal(&id1);
    assert!(
        goal1.is_some(),
        "First goal must persist across ledger advancements"
    );
    assert_eq!(goal1.unwrap().current_amount, 3000);

    let goal2 = client.get_goal(&id2);
    assert!(goal2.is_some(), "Second goal must persist");
    assert_eq!(goal2.unwrap().current_amount, 10000);

    // TTL should be fully refreshed
    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must remain >= 518,400 after repeated operations",
        ttl
    );
}

/// Verify that lock_goal extends instance TTL.
#[test]
fn test_instance_ttl_extended_on_lock_goal() {
    let env = Env::default();
    env.mock_all_auths();

    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 100,
        timestamp: 1000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();

    let goal_id = client.create_goal(
        &user,
        &String::from_str(&env, "Retirement"),
        &100000,
        &2000000000,
    );

    // Advance ledger past threshold
    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 510_000,
        timestamp: 510_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    // lock_goal calls extend_instance_ttl
    client.lock_goal(&user, &goal_id);

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= 518,400 after lock_goal",
        ttl
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Time & Ledger Drift Resilience Tests (#158)
//
// Assumptions documented here:
//  - Stellar ledger timestamps are monotonically increasing in production.
//  - is_goal_completed checks current_amount >= target_amount only; the
//    target_date field is informational and does not affect completion.
//  - execute_due_savings_schedules fires a schedule when
//    current_time >= schedule.next_due (inclusive boundary).
//  - After execution, next_due advances by the interval, preventing
//    re-execution even if ledger time were to regress (non-monotonic drift).
// ══════════════════════════════════════════════════════════════════════════

/// Passing target_date has no effect on is_goal_completed; the check is
/// purely current_amount vs target_amount.
#[test]
fn test_time_drift_is_goal_completed_depends_on_amount_not_time() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    let target_date = 5000u64;
    set_time(&env, 1000);

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Vacation"),
        &10000,
        &target_date,
    );

    // Under-funded before target_date
    assert!(!client.is_goal_completed(&goal_id));

    // Advance ledger to exactly target_date – still under-funded
    set_time(&env, target_date);
    assert!(!client.is_goal_completed(&goal_id));

    // Advance ledger past target_date – still under-funded
    set_time(&env, target_date + 1);
    assert!(!client.is_goal_completed(&goal_id));

    // Fund the goal fully after the deadline
    client.add_to_goal(&owner, &goal_id, &10000);
    assert!(
        client.is_goal_completed(&goal_id),
        "Goal must complete on amount alone, regardless of time"
    );
}

/// Goal completes as soon as funding is sufficient, even far before target_date.
#[test]
fn test_time_drift_is_goal_completed_early_funding() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 100);

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Emergency Fund"),
        &5000,
        &9_999_999, // far-future target_date
    );

    assert!(!client.is_goal_completed(&goal_id));
    client.add_to_goal(&owner, &goal_id, &5000);
    assert!(
        client.is_goal_completed(&goal_id),
        "Goal must complete before target_date when amount is reached"
    );
}

/// Schedule must NOT execute one second before next_due and MUST execute
/// exactly at next_due (inclusive boundary).
#[test]
fn test_time_drift_schedule_executes_at_exact_next_due() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "House"),
        &50000,
        &200000,
    );
    let next_due = 3000u64;
    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &next_due, &86400);

    // One second before due: must NOT execute
    set_time(&env, next_due - 1);
    let executed = client.execute_due_savings_schedules();
    assert_eq!(
        executed.len(),
        0,
        "Schedule must not execute one second before next_due"
    );
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 0, "No funds added before due date");

    // Exactly at next_due: must execute
    set_time(&env, next_due);
    let executed = client.execute_due_savings_schedules();
    assert_eq!(executed.len(), 1, "Schedule must execute exactly at next_due");
    assert_eq!(executed.get(0).unwrap(), schedule_id);
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 500);
}

/// After execution next_due advances; a subsequent call at a time still
/// before the new next_due must not re-execute the schedule.
/// Documents non-monotonic time assumption: next_due guards re-runs.
#[test]
fn test_time_drift_no_double_execution_after_next_due_advances() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Car"),
        &20000,
        &999999,
    );
    let next_due = 5000u64;
    let interval = 86400u64;
    client.create_savings_schedule(&owner, &goal_id, &1000, &next_due, &interval);

    // Execute at next_due – schedule advances to next_due + interval
    set_time(&env, next_due);
    let executed = client.execute_due_savings_schedules();
    assert_eq!(executed.len(), 1);

    // Time between old next_due and new next_due: no re-execution
    // (In production ledger time is monotonic; this also covers the case
    //  where execute is called repeatedly within the same window.)
    set_time(&env, next_due + 100);
    let executed_again = client.execute_due_savings_schedules();
    assert_eq!(
        executed_again.len(),
        0,
        "Schedule must not re-execute before the new next_due"
    );

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 1000, "Funds must be added exactly once");
}

/// A large forward jump past multiple intervals marks the correct missed_count
/// and advances next_due beyond all skipped intervals.
#[test]
fn test_time_drift_large_jump_marks_missed_count() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "Tuition"),
        &50000,
        &9999999,
    );
    let next_due = 2000u64;
    let interval = 86400u64;
    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &next_due, &interval);

    // Jump 3 full intervals past first due date
    set_time(&env, next_due + interval * 3 + 500);
    client.execute_due_savings_schedules();

    let schedule = client.get_savings_schedule(&schedule_id).unwrap();
    assert_eq!(
        schedule.missed_count, 3,
        "Three intervals were skipped; missed_count must be 3"
    );
    assert!(
        schedule.next_due > next_due + interval * 3,
        "next_due must have advanced past all skipped intervals"
    );
}
