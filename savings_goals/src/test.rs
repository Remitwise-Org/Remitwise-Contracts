#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::{
    testutils::{Address as AddressTrait, Events, Ledger, LedgerInfo},
    symbol_short, Address, Env, IntoVal, String, Symbol, TryFromVal, Vec as SorobanVec,
};

use testutils::set_ledger_time;

// Removed local set_time in favor of testutils::set_ledger_time

#[test]
fn test_create_goal_unique_ids_succeeds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    env.mock_all_auths();
    client.init();

    let name1 = String::from_str(&env, "Goal 1");
    let name2 = String::from_str(&env, "Goal 2");

    let id1 = client.create_goal(&user, &name1, &1000, &1735689600);
    let id2 = client.create_goal(&user, &name2, &2000, &1735689600);

    assert_ne!(id1, id2);
}

/// Documented behavior: past target dates are allowed (e.g. for backfill or
/// data migration). This test locks in that create_goal accepts a target_date
/// earlier than the current ledger timestamp and persists it as provided.
#[test]
fn test_create_goal_allows_past_target_date() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    // Move ledger time forward so our target_date is clearly in the past.
    set_ledger_time(&env, 1, 2_000_000_000);
    let past_target_date = 1_000_000_000u64;

    let name = String::from_str(&env, "Backfill Goal");
    let id = client.create_goal(&user, &name, &1000, &past_target_date);

    assert_eq!(id, 1);
    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.target_date, past_target_date);
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
    assert_eq!(
        all_goals.len(),
        1,
        "get_all_goals must still return the one goal"
    );

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

    let goal_names = ["G1", "G2", "G3"];
    for (i, &id) in ids.iter().enumerate() {
        let goal = client.get_goal(&id).unwrap();
        assert_eq!(goal.id, id);
        let expected_name = String::from_str(&env, goal_names[i]);
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
    assert!(res.is_err());
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
    assert!(res.is_err());
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
fn test_withdraw_from_goal_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Success"),
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
fn test_withdraw_from_goal_insufficient_balance() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Insufficient"),
        &1000,
        &2000000000,
    );

    client.unlock_goal(&user, &id);
    client.add_to_goal(&user, &id, &100);

    let res = client.try_withdraw_from_goal(&user, &id, &200);
    assert!(res.is_err());
}

#[test]
fn test_withdraw_from_goal_locked() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Locked"), &1000, &2000000000);

    client.add_to_goal(&user, &id, &500);
    let res = client.try_withdraw_from_goal(&user, &id, &100);
    assert!(res.is_err());
}

#[test]
fn test_withdraw_from_goal_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Unauthorized"),
        &1000,
        &2000000000,
    );

    client.unlock_goal(&user, &id);
    client.add_to_goal(&user, &id, &500);

    let res = client.try_withdraw_from_goal(&other, &id, &100);
    assert!(res.is_err());
}

#[test]
fn test_withdraw_from_goal_zero_amount_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Zero"), &1000, &2000000000);

    client.unlock_goal(&user, &id);
    client.add_to_goal(&user, &id, &500);
    let result = client.try_withdraw_from_goal(&user, &id, &0);
    assert!(result.is_err(), "Expected error for zero amount withdrawal");
}

#[test]
fn test_withdraw_from_goal_nonexistent_goal_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let result = client.try_withdraw_from_goal(&user, &999, &100);
    assert!(
        result.is_err(),
        "Expected error for nonexistent goal withdrawal"
    );
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
fn test_withdraw_full_balance() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Full"), &1000, &2000000000);

    client.unlock_goal(&user, &id);
    client.add_to_goal(&user, &id, &500);

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
fn test_set_time_lock_succeeds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();
    client.init();
    set_ledger_time(&env, 1, 1000);

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
    set_ledger_time(&env, 1, 1000);

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
    set_ledger_time(&env, 1, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &10000, &5000);

    client.add_to_goal(&owner, &goal_id, &5000);
    client.unlock_goal(&owner, &goal_id);
    client.set_time_lock(&owner, &goal_id, &3000);

    set_ledger_time(&env, 1, 3500);
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
    set_ledger_time(&env, 1, 1000);

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
    set_ledger_time(&env, 1, 1000);

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
    set_ledger_time(&env, 1, 1000);

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
    set_ledger_time(&env, 1, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &10000, &5000);

    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &3000, &0);

    set_ledger_time(&env, 1, 3500);
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
    set_ledger_time(&env, 1, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &10000, &5000);

    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &3000, &86400);

    set_ledger_time(&env, 1, 3500);
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
    set_ledger_time(&env, 1, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &10000, &5000);

    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &3000, &86400);

    set_ledger_time(&env, 1, 3000 + 86400 * 3 + 100);
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
    set_ledger_time(&env, 1, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Education"), &1000, &5000);

    client.create_savings_schedule(&owner, &goal_id, &1000, &3000, &0);

    set_ledger_time(&env, 1, 3500);
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
    assert!(res.is_err());
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
    assert!(res.is_err());
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
    assert!(res.is_err());
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
    assert!(res.is_err());
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

    let events = soroban_sdk::testutils::Events::all(&env.events());
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

    let events = soroban_sdk::testutils::Events::all(&env.events());
    let mut found_added_struct = false;
    let mut found_added_enum = false;

    for event in events.iter() {
        let topics = event.1;
        let topic0: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();

        if topic0 == symbol_short!("Remitwise") && topics.len() >= 4 {
            let action: Symbol = Symbol::try_from_val(&env, &topics.get(3).unwrap()).unwrap();
            if action == symbol_short!("funds_add") {
                let event_data: FundsAddedEvent =
                    FundsAddedEvent::try_from_val(&env, &event.2).unwrap();
                assert_eq!(event_data.goal_id, goal_id);
                assert_eq!(event_data.amount, 1000);
                found_added_struct = true;
            }
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

    let events = soroban_sdk::testutils::Events::all(&env.events());
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

    let events = soroban_sdk::testutils::Events::all(&env.events());
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

    let events = soroban_sdk::testutils::Events::all(&env.events());
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

    let events = soroban_sdk::testutils::Events::all(&env.events());
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
    let events = soroban_sdk::testutils::Events::all(&env.events());
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

fn setup_goals(env: &Env, client: &SavingsGoalContractClient, owner: &Address, count: u32) {
    for i in 0..count {
        client.create_goal(
            owner,
            &soroban_sdk::String::from_str(env, "Goal"),
            &(1000i128 * (i as i128 + 1)),
            &(env.ledger().timestamp() + 86400 * (i as u64 + 1)),
        );
    }
}

fn page_goal_ids(env: &Env, page: &GoalPage) -> soroban_sdk::Vec<u32> {
    let mut ids = soroban_sdk::Vec::new(env);
    for goal in page.items.iter() {
        ids.push_back(goal.id);
    }
    ids
}

#[test]
fn test_get_goals_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &id);
    let owner = Address::generate(&env);

    client.init();
    let page = client.get_goals(&owner, &0, &0);
    assert_eq!(page.count, 0);
    assert_eq!(page.next_cursor, 0);
    assert_eq!(page.items.len(), 0);
}

#[test]
fn test_get_goals_single_page() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &id);
    let owner = Address::generate(&env);

    client.init();
    setup_goals(&env, &client, &owner, 5);

    let page = client.get_goals(&owner, &0, &10);
    assert_eq!(page.count, 5);
    assert_eq!(page.next_cursor, 0);
}

#[test]
fn test_get_goals_multiple_pages() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &id);
    let owner = Address::generate(&env);

    client.init();
    setup_goals(&env, &client, &owner, 9);

    // Page 1
    let page1 = client.get_goals(&owner, &0, &4);
    assert_eq!(page1.count, 4);
    assert!(page1.next_cursor > 0);

    // Page 2
    let page2 = client.get_goals(&owner, &page1.next_cursor, &4);
    assert_eq!(page2.count, 4);
    assert!(page2.next_cursor > 0);

    // Page 3 (last)
    let page3 = client.get_goals(&owner, &page2.next_cursor, &4);
    assert_eq!(page3.count, 1);
    assert_eq!(page3.next_cursor, 0);
}

#[test]
fn test_get_goals_multi_owner_isolation() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &id);
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);

    client.init();
    setup_goals(&env, &client, &owner_a, 3);
    setup_goals(&env, &client, &owner_b, 4);

    let page_a = client.get_goals(&owner_a, &0, &20);
    assert_eq!(page_a.count, 3);
    for g in page_a.items.iter() {
        assert_eq!(g.owner, owner_a);
    }

    let page_b = client.get_goals(&owner_b, &0, &20);
    assert_eq!(page_b.count, 4);
}

#[test]
fn test_get_goals_cursor_is_exclusive() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &id);
    let owner = Address::generate(&env);

    client.init();
    setup_goals(&env, &client, &owner, 4);

    let first = client.get_goals(&owner, &0, &2);
    assert_eq!(first.count, 2);
    let last_id = first.items.get(1).unwrap().id;

    // cursor should be exclusive — next page should NOT include last_id
    let second = client.get_goals(&owner, &last_id, &2);
    for g in second.items.iter() {
        assert!(g.id > last_id, "cursor should be exclusive");
    }
}

#[test]
fn test_get_goals_rejects_invalid_cursor() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &id);
    let owner = Address::generate(&env);

    client.init();
    setup_goals(&env, &client, &owner, 4);

    let res = client.try_get_goals(&owner, &999_999, &2);
    assert!(res.is_err(), "non-zero cursor must exist for this owner");
}

#[test]
fn test_get_goals_rejects_cursor_from_another_owner() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &id);
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);

    client.init();
    setup_goals(&env, &client, &owner_a, 3);
    setup_goals(&env, &client, &owner_b, 2);

    let owner_b_first_page = client.get_goals(&owner_b, &0, &1);
    let foreign_cursor = owner_b_first_page.items.get(0).unwrap().id;
    let res = client.try_get_goals(&owner_a, &foreign_cursor, &2);
    assert!(res.is_err(), "cursor must be bound to the requested owner");
}

#[test]
fn test_get_goals_no_duplicate_or_skip_when_new_goals_added_between_pages() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &id);
    let owner = Address::generate(&env);

    client.init();
    setup_goals(&env, &client, &owner, 6);

    let page1 = client.get_goals(&owner, &0, &3);
    let page1_ids = page_goal_ids(&env, &page1);
    assert_eq!(page1_ids.get(0), Some(1));
    assert_eq!(page1_ids.get(1), Some(2));
    assert_eq!(page1_ids.get(2), Some(3));

    // Simulate concurrent writes between paged reads.
    setup_goals(&env, &client, &owner, 2);

    let page2 = client.get_goals(&owner, &page1.next_cursor, &3);
    let page2_ids = page_goal_ids(&env, &page2);
    assert_eq!(page2_ids.get(0), Some(4));
    assert_eq!(page2_ids.get(1), Some(5));
    assert_eq!(page2_ids.get(2), Some(6));

    let page3 = client.get_goals(&owner, &page2.next_cursor, &3);
    let page3_ids = page_goal_ids(&env, &page3);
    assert_eq!(page3_ids.get(0), Some(7));
    assert_eq!(page3_ids.get(1), Some(8));
    assert_eq!(page3.count, 2);
    assert_eq!(page3.next_cursor, 0);
}

#[test]
fn test_limit_zero_uses_default() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &id);
    let owner = Address::generate(&env);

    client.init();
    setup_goals(&env, &client, &owner, 3);
    let page = client.get_goals(&owner, &0, &0);
    assert_eq!(page.count, 3); // 3 < DEFAULT_PAGE_LIMIT so all returned
}

#[test]
fn test_get_all_goals_backward_compat() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &id);
    let owner = Address::generate(&env);

    client.init();
    setup_goals(&env, &client, &owner, 5);
    let all = client.get_all_goals(&owner);
    assert_eq!(all.len(), 5);
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_add_to_goal_non_owner_auth_failure() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &user,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_goal",
            args: (
                &user,
                String::from_str(&env, "Auth"),
                1000i128,
                2000000000u64,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let id = client.create_goal(&user, &String::from_str(&env, "Auth"), &1000, &2000000000);
    client.add_to_goal(&other, &id, &500);
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_withdraw_from_goal_non_owner_auth_failure() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &user,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_goal",
            args: (
                &user,
                String::from_str(&env, "Auth"),
                1000i128,
                2000000000u64,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let id = client.create_goal(&user, &String::from_str(&env, "Auth"), &1000, &2000000000);
    client.withdraw_from_goal(&other, &id, &100);
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_lock_goal_non_owner_auth_failure() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &user,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_goal",
            args: (
                &user,
                String::from_str(&env, "Auth"),
                1000i128,
                2000000000u64,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let id = client.create_goal(&user, &String::from_str(&env, "Auth"), &1000, &2000000000);
    client.lock_goal(&other, &id);
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_unlock_goal_non_owner_auth_failure() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &user,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_goal",
            args: (
                &user,
                String::from_str(&env, "Auth"),
                1000i128,
                2000000000u64,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let id = client.create_goal(&user, &String::from_str(&env, "Auth"), &1000, &2000000000);
    client.unlock_goal(&other, &id);
}

#[test]
fn test_get_all_goals_filters_by_owner() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);

    client.init();
    env.mock_all_auths();

    // Create two different owners
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);

    // Create goals for owner_a
    let goal_a1 = client.create_goal(
        &owner_a,
        &String::from_str(&env, "Goal A1"),
        &1000,
        &1735689600,
    );
    let goal_a2 = client.create_goal(
        &owner_a,
        &String::from_str(&env, "Goal A2"),
        &2000,
        &1735689600,
    );
    let goal_a3 = client.create_goal(
        &owner_a,
        &String::from_str(&env, "Goal A3"),
        &3000,
        &1735689600,
    );

    // Create goals for owner_b
    let goal_b1 = client.create_goal(
        &owner_b,
        &String::from_str(&env, "Goal B1"),
        &5000,
        &1735689600,
    );
    let goal_b2 = client.create_goal(
        &owner_b,
        &String::from_str(&env, "Goal B2"),
        &6000,
        &1735689600,
    );

    // Get all goals for owner_a
    let goals_a = client.get_all_goals(&owner_a);
    assert_eq!(goals_a.len(), 3, "Owner A should have exactly 3 goals");

    // Verify all goals returned for owner_a belong to owner_a
    for goal in goals_a.iter() {
        assert_eq!(
            goal.owner, owner_a,
            "Goal {} should belong to owner_a",
            goal.id
        );
    }

    // Verify goal IDs for owner_a are correct
    let goal_a_ids: std::vec::Vec<u32> = goals_a.iter().map(|g| g.id).collect();
    assert!(goal_a_ids.contains(&goal_a1), "Goals for A should contain goal_a1");
    assert!(goal_a_ids.contains(&goal_a2), "Goals for A should contain goal_a2");
    assert!(goal_a_ids.contains(&goal_a3), "Goals for A should contain goal_a3");
    assert!(goals_a.iter().any(|g| g.id == goal_a1), "Goals for A should contain goal_a1");
    assert!(goals_a.iter().any(|g| g.id == goal_a2), "Goals for A should contain goal_a2");
    assert!(goals_a.iter().any(|g| g.id == goal_a3), "Goals for A should contain goal_a3");

    // Get all goals for owner_b
    let goals_b = client.get_all_goals(&owner_b);
    assert_eq!(goals_b.len(), 2, "Owner B should have exactly 2 goals");

    // Verify all goals returned for owner_b belong to owner_b
    for goal in goals_b.iter() {
        assert_eq!(
            goal.owner, owner_b,
            "Goal {} should belong to owner_b",
            goal.id
        );
    }

    // Verify goal IDs for owner_b are correct
    let goal_b_ids: std::vec::Vec<u32> = goals_b.iter().map(|g| g.id).collect();
    assert!(goal_b_ids.contains(&goal_b1), "Goals for B should contain goal_b1");
    assert!(goal_b_ids.contains(&goal_b2), "Goals for B should contain goal_b2");
    assert!(goals_b.iter().any(|g| g.id == goal_b1), "Goals for B should contain goal_b1");
    assert!(goals_b.iter().any(|g| g.id == goal_b2), "Goals for B should contain goal_b2");

    // Verify that goal IDs between owner_a and owner_b are disjoint
    for goal_a in goals_a.iter() {
        assert!(
            !goals_b.iter().any(|gb| gb.id == goal_a.id),
            "Goal ID from owner A should not appear in owner B's goals"
        );
    }
}

    #[test]
    fn test_lock_goal_idempotent_already_locked() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        client.init();
        env.mock_all_auths();
        let id = client.create_goal(&user, &String::from_str(&env, "Idempotent Lock"), &1000, &2000000000);
        assert!(client.get_goal(&id).unwrap().locked);
        let result = client.lock_goal(&user, &id);
        assert!(result);
        assert!(client.get_goal(&id).unwrap().locked);
    }

    #[test]
    fn test_lock_goal_idempotent_no_duplicate_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        client.init();
        env.mock_all_auths();
        let id = client.create_goal(&user, &String::from_str(&env, "No Dup Lock"), &1000, &2000000000);
        client.unlock_goal(&user, &id);
        client.lock_goal(&user, &id);
        let events_after_first_lock = env.events().all().len();
        client.lock_goal(&user, &id);
        let events_after_second_lock = env.events().all().len();
        assert_eq!(events_after_first_lock, events_after_second_lock);
    }

    #[test]
    fn test_unlock_goal_idempotent_already_unlocked() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        client.init();
        env.mock_all_auths();
        let id = client.create_goal(&user, &String::from_str(&env, "Idempotent Unlock"), &1000, &2000000000);
        client.unlock_goal(&user, &id);
        assert!(!client.get_goal(&id).unwrap().locked);
        let result = client.unlock_goal(&user, &id);
        assert!(result);
        assert!(!client.get_goal(&id).unwrap().locked);
    }

    #[test]
    fn test_unlock_goal_idempotent_no_duplicate_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        client.init();
        env.mock_all_auths();
        let id = client.create_goal(&user, &String::from_str(&env, "No Dup Unlock"), &1000, &2000000000);
        client.unlock_goal(&user, &id);
        let events_after_first_unlock = env.events().all().len();
        client.unlock_goal(&user, &id);
        let events_after_second_unlock = env.events().all().len();
        assert_eq!(events_after_first_unlock, events_after_second_unlock);
    }

    #[test]
    fn test_lock_goal_many_repeated_calls_safe() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        client.init();
        env.mock_all_auths();
        let id = client.create_goal(&user, &String::from_str(&env, "Repeat Lock"), &1000, &2000000000);
        for _ in 0..5 {
            let result = client.lock_goal(&user, &id);
            assert!(result);
        }
        assert!(client.get_goal(&id).unwrap().locked);
    }

    #[test]
    fn test_unlock_goal_many_repeated_calls_safe() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        client.init();
        env.mock_all_auths();
        let id = client.create_goal(&user, &String::from_str(&env, "Repeat Unlock"), &1000, &2000000000);
        client.unlock_goal(&user, &id);
        for _ in 0..5 {
            let result = client.unlock_goal(&user, &id);
            assert!(result);
        }
        assert!(!client.get_goal(&id).unwrap().locked);
    }

    #[test]
    fn test_idempotent_unlock_does_not_bypass_time_lock() {
        let env = Env::default();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        env.mock_all_auths();
        set_ledger_time(&env, 1, 1000);
        let id = client.create_goal(&owner, &String::from_str(&env, "TimeLock"), &10000, &5000);
        client.add_to_goal(&owner, &id, &5000);
        client.unlock_goal(&owner, &id);
        client.set_time_lock(&owner, &id, &10000);
        client.unlock_goal(&owner, &id);
        let result = client.try_withdraw_from_goal(&owner, &id, &1000);
        assert!(result.is_err());
    }
// ============================================================================
// Snapshot schema version tests
//
// These tests verify that:
//  1. export_snapshot embeds the correct schema_version tag.
//  2. import_snapshot accepts schema_version within the supported range.
//  3. import_snapshot rejects a future (too-new) schema version.
//  4. import_snapshot rejects a past (too-old, below minimum) schema version.
//  5. import_snapshot rejects a tampered checksum regardless of version.
//  6. Full round-trip: exported data is faithfully restored after import.
// ============================================================================

/// export_snapshot must embed schema_version == SCHEMA_VERSION (currently 1).
#[test]
fn test_export_snapshot_contains_correct_schema_version() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    let _id = client.create_goal(
        &owner,
        &String::from_str(&env, "House"),
        &10000,
        &2000000000,
    );

    let snapshot = client.export_snapshot(&owner);
    assert_eq!(
        snapshot.schema_version, 1,
        "schema_version must equal SCHEMA_VERSION (1)"
    );
}

/// import_snapshot with the current schema version (1) must succeed.
#[test]
fn test_import_snapshot_current_schema_version_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Car"), &5000, &2000000000);

    let snapshot = client.export_snapshot(&owner);
    assert_eq!(snapshot.schema_version, 1);

    let ok = client.import_snapshot(&owner, &0, &snapshot);
    assert!(ok, "import with current schema version must succeed");
}

/// import_snapshot with schema_version higher than SCHEMA_VERSION must return
/// UnsupportedVersion (forward-compat rejection).
#[test]
fn test_import_snapshot_future_schema_version_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Trip"), &3000, &2000000000);

    let mut snapshot = client.export_snapshot(&owner);
    // Simulate a snapshot produced by a newer contract version.
    snapshot.schema_version = 999;

    let result = client.try_import_snapshot(&owner, &0, &snapshot);
    assert_eq!(
        result,
        Err(Ok(SavingsGoalError::UnsupportedVersion)),
        "future schema_version must be rejected"
    );
}

/// import_snapshot with schema_version = 0 (below MIN_SUPPORTED_SCHEMA_VERSION)
/// must return UnsupportedVersion (backward-compat rejection).
#[test]
fn test_import_snapshot_too_old_schema_version_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(
        &owner,
        &String::from_str(&env, "Education"),
        &8000,
        &2000000000,
    );

    let mut snapshot = client.export_snapshot(&owner);
    // Simulate a snapshot too old to be safely imported.
    snapshot.schema_version = 0;

    let result = client.try_import_snapshot(&owner, &0, &snapshot);
    assert_eq!(
        result,
        Err(Ok(SavingsGoalError::UnsupportedVersion)),
        "schema_version below minimum must be rejected"
    );
}

/// import_snapshot with a tampered checksum must return ChecksumMismatch even
/// when the schema_version is valid.
#[test]
fn test_import_snapshot_tampered_checksum_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(
        &owner,
        &String::from_str(&env, "Savings"),
        &2000,
        &2000000000,
    );

    let mut snapshot = client.export_snapshot(&owner);
    snapshot.checksum = snapshot.checksum.wrapping_add(1);

    let result = client.try_import_snapshot(&owner, &0, &snapshot);
    assert_eq!(
        result,
        Err(Ok(SavingsGoalError::ChecksumMismatch)),
        "tampered checksum must be rejected"
    );
}

/// Full export → import round-trip: goal data is faithfully restored.
#[test]
fn test_snapshot_export_import_roundtrip_restores_goals() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    let id1 = client.create_goal(
        &owner,
        &String::from_str(&env, "Fund A"),
        &5000,
        &2000000000,
    );
    let id2 = client.create_goal(
        &owner,
        &String::from_str(&env, "Fund B"),
        &8000,
        &2000000000,
    );
    client.add_to_goal(&owner, &id1, &1500);

    let snapshot = client.export_snapshot(&owner);
    assert_eq!(snapshot.schema_version, 1);
    assert_eq!(snapshot.goals.len(), 2);

    let ok = client.import_snapshot(&owner, &0, &snapshot);
    assert!(ok, "round-trip import must succeed");

    let restored1 = client.get_goal(&id1).expect("goal 1 must survive import");
    assert_eq!(restored1.target_amount, 5000);
    assert_eq!(restored1.current_amount, 1500);

    let restored2 = client.get_goal(&id2).expect("goal 2 must survive import");
    assert_eq!(restored2.target_amount, 8000);
}

/// schema_version boundary: version exactly at MIN_SUPPORTED_SCHEMA_VERSION (1)
/// must be accepted.
#[test]
fn test_import_snapshot_min_supported_version_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(
        &owner,
        &String::from_str(&env, "Min Version"),
        &1000,
        &2000000000,
    );

    let snapshot = client.export_snapshot(&owner);
    // schema_version is already 1 == MIN_SUPPORTED_SCHEMA_VERSION.
    assert_eq!(snapshot.schema_version, 1);

    let ok = client.import_snapshot(&owner, &0, &snapshot);
    assert!(
        ok,
        "snapshot at MIN_SUPPORTED_SCHEMA_VERSION must be accepted"
    );
}

// ============================================================================
// Extended snapshot import compatibility tests
//
// Covers:
//  - Empty snapshot (zero goals) import
//  - Malformed payload: tampered next_id causes checksum mismatch
//  - Malformed payload: zeroed checksum on non-empty snapshot
//  - Malformed payload: schema_version = u32::MAX rejected
//  - Nonce replay: reusing a consumed nonce must panic
//  - Nonce wrong value: supplying an incorrect nonce must panic
//  - Ownership remap: snapshot goals owned by a different address are preserved
//  - Multi-owner snapshot: all goal owners survive import unchanged
//  - Import overwrites existing state: prior goals are replaced
//  - Audit log: import appends a success entry to the audit log
//  - Export event: export_snapshot emits the snap_exp event
//  - Sequential imports: nonce increments correctly across multiple imports
// ============================================================================

/// Importing an empty snapshot (zero goals) must succeed and clear existing goals.
///
/// # Security note
/// An empty import is a valid migration step (e.g. contract reset). The
/// checksum for an empty payload must still be validated to prevent spoofing.
#[test]
fn test_import_empty_snapshot_succeeds_and_clears_goals() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Old Goal"), &5000, &2000000000);

    // Build an empty snapshot manually with a valid checksum.
    // checksum = (version + next_id) * 31 = (1 + 0) * 31 = 31
    let empty_snapshot = GoalsExportSnapshot {
        schema_version: 1,
        checksum: 31,
        next_id: 0,
        goals: Vec::new(&env),
    };

    let ok = client.import_snapshot(&owner, &0, &empty_snapshot);
    assert!(ok, "empty snapshot import must succeed");

    // After import, the old goal must no longer exist.
    assert!(
        client.get_goal(&1).is_none(),
        "old goal must be cleared after empty snapshot import"
    );
}

/// Malformed payload: tampering with `next_id` while keeping the original
/// checksum must be rejected with ChecksumMismatch.
///
/// # Security note
/// `next_id` is part of the checksum input. Any mutation of it without
/// recomputing the checksum must be detected.
#[test]
fn test_import_snapshot_tampered_next_id_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Goal"), &3000, &2000000000);

    let mut snapshot = client.export_snapshot(&owner);
    // Mutate next_id without updating checksum — payload is now malformed.
    snapshot.next_id = snapshot.next_id.wrapping_add(99);

    let result = client.try_import_snapshot(&owner, &0, &snapshot);
    assert_eq!(
        result,
        Err(Ok(SavingsGoalError::ChecksumMismatch)),
        "tampered next_id must be detected via checksum"
    );
}

/// Malformed payload: a zeroed checksum on a non-empty snapshot must be
/// rejected with ChecksumMismatch.
///
/// # Security note
/// Checksum = 0 is only valid for a specific (version, next_id, goals)
/// combination. For any non-trivial snapshot it must be rejected.
#[test]
fn test_import_snapshot_zero_checksum_on_nonempty_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Goal"), &4000, &2000000000);

    let mut snapshot = client.export_snapshot(&owner);
    snapshot.checksum = 0;

    let result = client.try_import_snapshot(&owner, &0, &snapshot);
    assert_eq!(
        result,
        Err(Ok(SavingsGoalError::ChecksumMismatch)),
        "zero checksum on non-empty snapshot must be rejected"
    );
}

/// Malformed payload: schema_version = u32::MAX must be rejected as
/// UnsupportedVersion (far-future version guard).
#[test]
fn test_import_snapshot_max_u32_schema_version_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Goal"), &1000, &2000000000);

    let mut snapshot = client.export_snapshot(&owner);
    snapshot.schema_version = u32::MAX;

    let result = client.try_import_snapshot(&owner, &0, &snapshot);
    assert_eq!(
        result,
        Err(Ok(SavingsGoalError::UnsupportedVersion)),
        "schema_version = u32::MAX must be rejected"
    );
}

/// Nonce replay: after a successful import the nonce is incremented.
/// Reusing the old nonce (0) on a second import must panic.
///
/// # Security note
/// Nonce replay protection prevents an attacker from replaying a captured
/// import transaction. Each successful import must consume the nonce.
#[test]
#[should_panic]
fn test_import_snapshot_nonce_replay_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Goal"), &1000, &2000000000);

    let snapshot = client.export_snapshot(&owner);

    // First import with nonce 0 — succeeds and increments nonce to 1.
    let ok = client.import_snapshot(&owner, &0, &snapshot);
    assert!(ok);

    // Second import with the same nonce 0 — must panic (nonce mismatch).
    client.import_snapshot(&owner, &0, &snapshot);
}

/// Nonce wrong value: supplying an incorrect nonce must panic before any
/// state mutation occurs.
///
/// # Security note
/// The nonce check is the first guard in import_snapshot. An incorrect nonce
/// must abort the call immediately, leaving state unchanged.
#[test]
#[should_panic]
fn test_import_snapshot_wrong_nonce_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Goal"), &1000, &2000000000);

    let snapshot = client.export_snapshot(&owner);
    // Nonce is 0 but we supply 42 — must panic.
    client.import_snapshot(&owner, &42, &snapshot);
}

/// Sequential imports: nonce increments correctly across multiple successful
/// imports, and each import with the correct nonce succeeds.
#[test]
fn test_import_snapshot_sequential_nonce_increments() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Goal"), &1000, &2000000000);

    let snapshot = client.export_snapshot(&owner);

    // Import 1: nonce 0 → nonce becomes 1
    assert!(client.import_snapshot(&owner, &0, &snapshot));
    assert_eq!(client.get_nonce(&owner), 1, "nonce must be 1 after first import");

    // Import 2: nonce 1 → nonce becomes 2
    assert!(client.import_snapshot(&owner, &1, &snapshot));
    assert_eq!(client.get_nonce(&owner), 2, "nonce must be 2 after second import");
}

/// Ownership remap: importing a snapshot whose goals are owned by a different
/// address must succeed. The import caller authorizes the operation; goal
/// ownership inside the snapshot is preserved as-is.
///
/// # Security note
/// import_snapshot does not re-assign goal ownership. The caller is
/// responsible for ensuring the snapshot content is trusted. This test
/// documents and locks in that behavior.
#[test]
fn test_import_snapshot_preserves_original_goal_owner() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let original_owner = Address::generate(&env);
    let importer = Address::generate(&env);

    client.init();
    client.create_goal(
        &original_owner,
        &String::from_str(&env, "Owned Goal"),
        &7000,
        &2000000000,
    );

    // Export as original_owner, then import as a different caller (importer).
    let snapshot = client.export_snapshot(&original_owner);
    let ok = client.import_snapshot(&importer, &0, &snapshot);
    assert!(ok, "import by a different caller must succeed");

    // Goal ownership must remain with original_owner, not importer.
    let goal = client.get_goal(&1).expect("goal must exist after import");
    assert_eq!(
        goal.owner, original_owner,
        "goal owner must be preserved from snapshot, not remapped to importer"
    );
}

/// Multi-owner snapshot: a snapshot containing goals from multiple owners
/// must restore all goals with their original owners intact.
#[test]
fn test_import_snapshot_multi_owner_goals_preserved() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init();
    let id_a = client.create_goal(&owner_a, &String::from_str(&env, "A Goal"), &3000, &2000000000);
    let id_b = client.create_goal(&owner_b, &String::from_str(&env, "B Goal"), &6000, &2000000000);

    // Admin exports the full snapshot (all goals regardless of owner).
    let snapshot = client.export_snapshot(&admin);
    assert_eq!(snapshot.goals.len(), 2, "snapshot must contain both goals");

    // Re-import as admin.
    let ok = client.import_snapshot(&admin, &0, &snapshot);
    assert!(ok);

    let goal_a = client.get_goal(&id_a).expect("goal A must survive import");
    let goal_b = client.get_goal(&id_b).expect("goal B must survive import");

    assert_eq!(goal_a.owner, owner_a, "goal A owner must be preserved");
    assert_eq!(goal_b.owner, owner_b, "goal B owner must be preserved");
}

/// Import overwrites existing state: goals present before import that are not
/// in the snapshot must be absent after import.
///
/// # Security note
/// import_snapshot is a full-state replacement, not a merge. Callers must
/// ensure the snapshot is complete to avoid unintended data loss.
#[test]
fn test_import_snapshot_overwrites_existing_goals() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    // Create goal 1 and export it.
    client.create_goal(&owner, &String::from_str(&env, "Keep"), &1000, &2000000000);
    let snapshot = client.export_snapshot(&owner);

    // Create goal 2 after the snapshot was taken.
    client.create_goal(&owner, &String::from_str(&env, "Discard"), &2000, &2000000000);
    assert!(client.get_goal(&2).is_some(), "goal 2 must exist before import");

    // Import the earlier snapshot — goal 2 must be gone.
    let ok = client.import_snapshot(&owner, &0, &snapshot);
    assert!(ok);

    assert!(client.get_goal(&1).is_some(), "goal 1 must survive import");
    assert!(
        client.get_goal(&2).is_none(),
        "goal 2 must be absent after import of older snapshot"
    );
}

/// Audit log: a successful import must append a success entry to the audit log.
///
/// # Security note
/// Every import must be traceable. The audit log provides an immutable record
/// of who imported what and whether it succeeded.
#[test]
fn test_import_snapshot_appends_success_audit_entry() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Goal"), &1000, &2000000000);

    let snapshot = client.export_snapshot(&owner);
    client.import_snapshot(&owner, &0, &snapshot);

    let log = client.get_audit_log(&0, &10);
    assert!(!log.is_empty(), "audit log must not be empty after import");

    // The last entry must record a successful import.
    let last = log.get(log.len() - 1).expect("audit log must have entries");
    assert!(last.success, "last audit entry must be a success");
}

/// Failed import (bad checksum) must append a failure entry to the audit log.
///
/// # Security note
/// Failed import attempts must also be logged so operators can detect
/// tampering or misconfigured migration tooling.
#[test]
fn test_import_snapshot_failed_checksum_appends_failure_audit_entry() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Goal"), &1000, &2000000000);

    let mut snapshot = client.export_snapshot(&owner);
    snapshot.checksum = snapshot.checksum.wrapping_add(1);

    let _ = client.try_import_snapshot(&owner, &0, &snapshot);

    let log = client.get_audit_log(&0, &10);
    assert!(!log.is_empty(), "audit log must not be empty after failed import");

    let last = log.get(log.len() - 1).expect("audit log must have entries");
    assert!(!last.success, "last audit entry must record failure");
}

/// export_snapshot must emit the (goals, snap_exp) event with the schema version.
#[test]
fn test_export_snapshot_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Goal"), &1000, &2000000000);

    client.export_snapshot(&owner);

    let events = env.events().all();
    let found = events.iter().any(|e| {
        let topics = e.1;
        if topics.len() < 2 {
            return false;
        }
        let t0 = Symbol::try_from_val(&env, &topics.get(0).unwrap());
        let t1 = Symbol::try_from_val(&env, &topics.get(1).unwrap());
        matches!((t0, t1), (Ok(a), Ok(b))
            if a == symbol_short!("goals") && b == symbol_short!("snap_exp"))
    });
    assert!(found, "export_snapshot must emit (goals, snap_exp) event");
}

/// Version transition: importing a snapshot at schema_version = 2 when
/// SCHEMA_VERSION = 1 must be rejected (forward-compat guard).
/// This test documents the expected behavior when a future contract version
/// produces a snapshot that an older contract cannot safely consume.
#[test]
fn test_import_snapshot_version_2_rejected_by_v1_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Goal"), &1000, &2000000000);

    let mut snapshot = client.export_snapshot(&owner);
    // Simulate a snapshot produced by a v2 contract.
    snapshot.schema_version = 2;

    let result = client.try_import_snapshot(&owner, &0, &snapshot);
    assert_eq!(
        result,
        Err(Ok(SavingsGoalError::UnsupportedVersion)),
        "schema_version 2 must be rejected by a v1 contract"
    );
}

/// Round-trip with locked goal: a locked goal must remain locked after import.
#[test]
fn test_import_snapshot_preserves_locked_state() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    let id = client.create_goal(&owner, &String::from_str(&env, "Locked"), &1000, &2000000000);
    // Goals are locked by default; verify before export.
    assert!(client.get_goal(&id).unwrap().locked);

    let snapshot = client.export_snapshot(&owner);
    client.import_snapshot(&owner, &0, &snapshot);

    let restored = client.get_goal(&id).expect("goal must exist after import");
    assert!(restored.locked, "locked state must be preserved through import");
}

/// Round-trip with time-lock: unlock_date must survive export → import.
#[test]
fn test_import_snapshot_preserves_time_lock() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    set_ledger_time(&env, 1, 1000);
    let id = client.create_goal(&owner, &String::from_str(&env, "TimeLocked"), &1000, &5000);
    client.set_time_lock(&owner, &id, &9999);

    let snapshot = client.export_snapshot(&owner);
    client.import_snapshot(&owner, &0, &snapshot);

    let restored = client.get_goal(&id).expect("goal must exist after import");
    assert_eq!(
        restored.unlock_date,
        Some(9999),
        "unlock_date must be preserved through import"
    );
}

/// Malformed payload: tampering with goal amounts while keeping the original
/// checksum must be rejected with ChecksumMismatch.
///
/// # Security note
/// Goal amounts are included in the checksum. Any mutation of goal data
/// without recomputing the checksum must be detected.
#[test]
fn test_import_snapshot_tampered_goal_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.init();
    client.create_goal(&owner, &String::from_str(&env, "Goal"), &5000, &2000000000);
    client.add_to_goal(&owner, &1, &2000);

    let mut snapshot = client.export_snapshot(&owner);

    // Mutate the first goal's target_amount without updating checksum.
    // soroban Vec has no set(); rebuild by iterating and replacing index 0.
    let mut goals_vec = Vec::new(&env);
    for (i, g) in snapshot.goals.iter().enumerate() {
        if i == 0 {
            let mut tampered = g.clone();
            tampered.target_amount = tampered.target_amount.wrapping_add(1);
            goals_vec.push_back(tampered);
        } else {
            goals_vec.push_back(g);
        }
    }
    snapshot.goals = goals_vec;

    let result = client.try_import_snapshot(&owner, &0, &snapshot);
    assert_eq!(
        result,
        Err(Ok(SavingsGoalError::ChecksumMismatch)),
        "tampered goal amount must be detected via checksum"
    );
}

#[test]
fn test_withdraw_time_lock_boundaries() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    client.init();
    
    let base_time = 1000;
    set_ledger_time(&env, 1, base_time);

    let unlock_date = 5000;
    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Time Lock Boundary"), &10000, &unlock_date);

    client.add_to_goal(&owner, &goal_id, &5000);
    client.unlock_goal(&owner, &goal_id);
    client.set_time_lock(&owner, &goal_id, &unlock_date);

    // 1. Test withdrawal at unlock_date - 1 (should fail)
    set_ledger_time(&env, 1, unlock_date - 1);
    let result = client.try_withdraw_from_goal(&owner, &goal_id, &1000);
    assert!(result.is_err(), "Withdrawal should fail before unlock_date");

    // 2. Test withdrawal at unlock_date (should succeed)
    set_ledger_time(&env, 1, unlock_date);
    let new_amount = client.withdraw_from_goal(&owner, &goal_id, &1000);
    assert_eq!(new_amount, 4000, "Withdrawal should succeed exactly at unlock_date");

    // 3. Test withdrawal at unlock_date + 1 (should succeed)
    set_ledger_time(&env, 1, unlock_date + 1);
    let final_amount = client.withdraw_from_goal(&owner, &goal_id, &1000);
    assert_eq!(final_amount, 3000, "Withdrawal should succeed after unlock_date");
}

#[test]
fn test_savings_schedule_drift_and_missed_intervals() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    client.init();
    
    let base_time = 1000;
    set_ledger_time(&env, 1, base_time);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Schedule Drift"), &10000, &5000);
    
    let amount = 500;
    let next_due = 3000;
    let interval = 86400; // 1 day
    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &amount, &next_due, &interval);

    // 1. Advance time past next_due + interval * 2 + 100 (simulating significant drift/delay)
    // 3000 + 172800 + 100 = 175900
    let current_time = next_due + interval * 2 + 100;
    set_ledger_time(&env, 1, current_time);
    
    let executed_ids = client.execute_due_savings_schedules();
    assert_eq!(executed_ids.len(), 1);
    assert_eq!(executed_ids.get(0).unwrap(), schedule_id);

    let schedule = client.get_savings_schedule(&schedule_id).unwrap();
    // It should have executed once (for the first due date) and missed 2 subsequent ones
    assert_eq!(schedule.missed_count, 2, "Should have marked 2 intervals as missed");
    
    // next_due should be set to the next FUTURE interval relative to current_time
    // Original: 3000
    // +1: 89400
    // +2: 175800
    // +3: 262200 (This is the next future one after 175900)
    assert_eq!(schedule.next_due, 262200, "next_due should anchor to the next future interval");

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, amount, "Only one execution should have happened");
}

#[test]
fn test_savings_schedule_exact_timestamp_execution() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    client.init();
    
    let base_time = 1000;
    set_ledger_time(&env, 1, base_time);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Exact Schedule"), &10000, &5000);
    
    let next_due = 3000;
    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &next_due, &0); // non-recurring

    // 1. Test at next_due - 1 (should NOT execute)
    set_ledger_time(&env, 1, next_due - 1);
    let executed_ids = client.execute_due_savings_schedules();
    assert_eq!(executed_ids.len(), 0, "Schedule should not execute before next_due");

    // 2. Test at next_due (should execute)
    set_ledger_time(&env, 1, next_due);
    let executed_ids = client.execute_due_savings_schedules();
    assert_eq!(executed_ids.len(), 1, "Schedule should execute exactly at next_due");
    assert_eq!(executed_ids.get(0).unwrap(), schedule_id);

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 500);
}

// ============================================================================
// Savings schedule duplicate-execution / idempotency tests
//
// These tests verify that execute_due_savings_schedules cannot credit a goal
// more than once for the same due window, regardless of how many times the
// function is invoked at the same ledger timestamp.
// ============================================================================

/// Calling execute_due_savings_schedules twice at the same ledger timestamp
/// for a one-shot (non-recurring) schedule must credit the goal exactly once.
///
/// Security: a one-shot schedule is deactivated (`active = false`) after the
/// first execution.  The second call must be a no-op and must not alter the
/// goal balance.
#[test]
fn test_execute_oneshot_schedule_idempotent() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Emergency"), &5000, &9999);
    // One-shot schedule: interval = 0
    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &3000, &0);

    // Advance time past the due date; both calls share the same timestamp.
    set_ledger_time(&env, 2, 3500);

    let first = client.execute_due_savings_schedules();
    let second = client.execute_due_savings_schedules();

    // First call must have executed the schedule.
    assert_eq!(first.len(), 1, "First call should execute one schedule");
    assert_eq!(first.get(0).unwrap(), schedule_id);

    // Second call must be a no-op (schedule is inactive after first execution).
    assert_eq!(second.len(), 0, "Second call must not re-execute the schedule");

    // Goal balance must reflect exactly one credit.
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 500, "Goal must be credited exactly once");

    // Schedule must be inactive.
    let schedule = client.get_savings_schedule(&schedule_id).unwrap();
    assert!(!schedule.active, "One-shot schedule must be inactive after execution");
}

/// Calling execute_due_savings_schedules twice at the same ledger timestamp
/// for a recurring schedule must credit the goal exactly once per due window.
///
/// Security: after the first execution `next_due` is advanced past
/// `current_time`, so the second call sees `next_due > current_time` and the
/// idempotency guard (`last_executed >= next_due_original`) both independently
/// prevent re-execution.  This test confirms neither protection is bypassed.
#[test]
fn test_execute_recurring_schedule_idempotent() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Vacation"), &10000, &99999);
    // Recurring schedule with a 1-day interval.
    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &200, &3000, &86400);

    set_ledger_time(&env, 2, 3500);

    let first = client.execute_due_savings_schedules();
    let second = client.execute_due_savings_schedules();

    // First call must execute once.
    assert_eq!(first.len(), 1, "First call should execute one schedule");
    assert_eq!(first.get(0).unwrap(), schedule_id);

    // Second call must be a no-op.
    assert_eq!(second.len(), 0, "Second call must not re-execute the schedule");

    // Goal balance must reflect exactly one credit.
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 200, "Goal must be credited exactly once");

    // Schedule must remain active with next_due advanced past current_time.
    let schedule = client.get_savings_schedule(&schedule_id).unwrap();
    assert!(schedule.active, "Recurring schedule must stay active");
    assert!(
        schedule.next_due > 3500,
        "next_due must be advanced past current_time after execution"
    );
    // last_executed must record when the schedule ran.
    assert_eq!(
        schedule.last_executed,
        Some(3500),
        "last_executed must be set to the execution timestamp"
    );
}

/// Executing a schedule and then calling execute again at a later timestamp
/// (within the next interval) must produce exactly one additional credit.
///
/// This confirms that after `next_due` is advanced the schedule correctly
/// fires again in the following window and does not double-fire.
#[test]
fn test_execute_recurring_fires_again_next_window() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Pension"), &10000, &99999);
    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &300, &3000, &1000);

    // First window: execute at t=3500 (past due t=3000)
    set_ledger_time(&env, 2, 3500);
    let first = client.execute_due_savings_schedules();
    assert_eq!(first.len(), 1);

    // Goal has one credit.
    let goal_after_first = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal_after_first.current_amount, 300);

    // Second window: execute at t=4500 (past advanced next_due t=4000)
    set_ledger_time(&env, 3, 4500);
    let second = client.execute_due_savings_schedules();
    assert_eq!(second.len(), 1, "Second window must execute once");
    assert_eq!(second.get(0).unwrap(), schedule_id);

    // Goal has two credits (not three or more).
    let goal_after_second = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal_after_second.current_amount, 600, "Goal must have exactly two credits");
}

/// Verifies that `last_executed` is always set to the ledger timestamp at the
/// moment of execution, not to `next_due` or any other derived value.
///
/// This is required for the idempotency guard (`last_executed >= next_due`) to
/// function correctly when `current_time > next_due` (i.e. the execution was
/// late).
#[test]
fn test_last_executed_set_to_current_time() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Housing"), &10000, &99999);
    // Due at 3000, but we execute late at 5000.
    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &100, &3000, &0);

    set_ledger_time(&env, 2, 5000);
    client.execute_due_savings_schedules();

    let schedule = client.get_savings_schedule(&schedule_id).unwrap();
    assert_eq!(
        schedule.last_executed,
        Some(5000),
        "last_executed must equal current_time (5000), not next_due (3000)"
    );
}

#[test]
fn test_add_tags_to_goal_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let goal_id = client.create_goal(&user, &String::from_str(&env, "Tagged"), &1000, &2000000000);

    let mut tags = SorobanVec::new(&env);
    tags.push_back(String::from_str(&env, "urgent"));

    let res = client.try_add_tags_to_goal(&other, &goal_id, &tags);
    assert!(res.is_err());
}

#[test]
fn test_remove_tags_from_goal_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let goal_id = client.create_goal(&user, &String::from_str(&env, "Tagged"), &1000, &2000000000);
    let mut tags = SorobanVec::new(&env);
    tags.push_back(String::from_str(&env, "urgent"));
    client.add_tags_to_goal(&user, &goal_id, &tags);

    let res = client.try_remove_tags_from_goal(&other, &goal_id, &tags);
    assert!(res.is_err());
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_add_tags_to_goal_non_owner_auth_failure() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &user,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_goal",
            args: (
                &user,
                String::from_str(&env, "Auth"),
                1000i128,
                2000000000u64,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let goal_id = client.create_goal(&user, &String::from_str(&env, "Auth"), &1000, &2000000000);
    let mut tags = SorobanVec::new(&env);
    tags.push_back(String::from_str(&env, "urgent"));
    client.add_tags_to_goal(&other, &goal_id, &tags);
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_remove_tags_from_goal_non_owner_auth_failure() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &user,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_goal",
            args: (
                &user,
                String::from_str(&env, "Auth"),
                1000i128,
                2000000000u64,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let goal_id = client.create_goal(&user, &String::from_str(&env, "Auth"), &1000, &2000000000);
    let mut tags = SorobanVec::new(&env);
    tags.push_back(String::from_str(&env, "urgent"));
    client.add_tags_to_goal(&user, &goal_id, &tags);
    client.remove_tags_from_goal(&other, &goal_id, &tags);
}

#[test]
#[should_panic(expected = "Tags cannot be empty")]
fn test_add_tags_to_goal_empty_tags_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let goal_id = client.create_goal(&user, &String::from_str(&env, "Empty"), &1000, &2000000000);
    let tags = SorobanVec::new(&env);
    client.add_tags_to_goal(&user, &goal_id, &tags);
}

#[test]
#[should_panic(expected = "Tag must be between 1 and 32 characters")]
fn test_add_tags_to_goal_invalid_tag_length_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let goal_id = client.create_goal(&user, &String::from_str(&env, "InvalidTag"), &1000, &2000000000);

    let mut tags = SorobanVec::new(&env);
    tags.push_back(String::from_str(
        &env,
        "this-tag-is-definitely-longer-than-thirty-two-chars",
    ));
    client.add_tags_to_goal(&user, &goal_id, &tags);
}

#[test]
#[should_panic(expected = "Tag must be between 1 and 32 characters")]
fn test_add_tags_to_goal_empty_string_tag_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let goal_id = client.create_goal(
        &user,
        &String::from_str(&env, "InvalidEmptyTag"),
        &1000,
        &2000000000,
    );

    let mut tags = SorobanVec::new(&env);
    tags.push_back(String::from_str(&env, ""));
    client.add_tags_to_goal(&user, &goal_id, &tags);
}

#[test]
#[should_panic(expected = "Goal not found")]
fn test_add_tags_to_goal_nonexistent_goal_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let mut tags = SorobanVec::new(&env);
    tags.push_back(String::from_str(&env, "urgent"));
    client.add_tags_to_goal(&user, &999, &tags);
}

#[test]
#[should_panic(expected = "Goal not found")]
fn test_remove_tags_from_goal_nonexistent_goal_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let mut tags = SorobanVec::new(&env);
    tags.push_back(String::from_str(&env, "urgent"));
    client.remove_tags_from_goal(&user, &999, &tags);
}

#[test]
fn test_add_and_remove_tags_to_goal_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let goal_id = client.create_goal(&user, &String::from_str(&env, "Travel"), &1000, &2000000000);

    let mut add_tags = SorobanVec::new(&env);
    add_tags.push_back(String::from_str(&env, "urgent"));
    add_tags.push_back(String::from_str(&env, "family"));
    client.add_tags_to_goal(&user, &goal_id, &add_tags);

    let goal_after_add = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal_after_add.tags.len(), 2);
    assert_eq!(goal_after_add.tags.get(0).unwrap(), String::from_str(&env, "urgent"));
    assert_eq!(goal_after_add.tags.get(1).unwrap(), String::from_str(&env, "family"));

    let mut remove_tags = SorobanVec::new(&env);
    remove_tags.push_back(String::from_str(&env, "urgent"));
    client.remove_tags_from_goal(&user, &goal_id, &remove_tags);

    let goal_after_remove = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal_after_remove.tags.len(), 1);
    assert_eq!(
        goal_after_remove.tags.get(0).unwrap(),
        String::from_str(&env, "family")
    );
}

#[test]
fn test_add_tags_to_goal_duplicates_allowed() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let goal_id =
        client.create_goal(&user, &String::from_str(&env, "DuplicateTags"), &1000, &2000000000);

    let mut tags = SorobanVec::new(&env);
    tags.push_back(String::from_str(&env, "duplicate"));
    tags.push_back(String::from_str(&env, "duplicate"));
    client.add_tags_to_goal(&user, &goal_id, &tags);

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.tags.len(), 2);
    assert_eq!(
        goal.tags.get(0).unwrap(),
        String::from_str(&env, "duplicate")
    );
    assert_eq!(
        goal.tags.get(1).unwrap(),
        String::from_str(&env, "duplicate")
    );
}

#[test]
fn test_remove_nonexistent_tag_keeps_existing_tags() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let goal_id = client.create_goal(&user, &String::from_str(&env, "Tags"), &1000, &2000000000);

    let mut original_tags = SorobanVec::new(&env);
    original_tags.push_back(String::from_str(&env, "rent"));
    client.add_tags_to_goal(&user, &goal_id, &original_tags);

    let mut remove_tags = SorobanVec::new(&env);
    remove_tags.push_back(String::from_str(&env, "food"));
    client.remove_tags_from_goal(&user, &goal_id, &remove_tags);

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.tags.len(), 1);
    assert_eq!(goal.tags.get(0).unwrap(), String::from_str(&env, "rent"));
}

#[test]
fn test_tag_operations_emit_events() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let goal_id = client.create_goal(&user, &String::from_str(&env, "Events"), &1000, &2000000000);

    let mut tags = SorobanVec::new(&env);
    tags.push_back(String::from_str(&env, "urgent"));
    client.add_tags_to_goal(&user, &goal_id, &tags);
    client.remove_tags_from_goal(&user, &goal_id, &tags);

    let events = env.events().all();
    let mut found_tags_add = false;
    let mut found_tags_rem = false;

    for event in events.iter() {
        let topics = event.1;
        if topics.len() < 2 {
            continue;
        }

        let topic0: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        let topic1 = Symbol::try_from_val(&env, &topics.get(1).unwrap());
        if topic1.is_err() {
            continue;
        }
        let topic1 = topic1.unwrap();

        if topic0 == symbol_short!("savings") && topic1 == symbol_short!("tags_add") {
            found_tags_add = true;
        }
        if topic0 == symbol_short!("savings") && topic1 == symbol_short!("tags_rem") {
            found_tags_rem = true;
        }
    }

    assert!(found_tags_add, "tags_add event was not emitted");
    assert!(found_tags_rem, "tags_rem event was not emitted");
}

// ============================================================================
// Savings schedule duplicate-execution / idempotency tests
//
// These tests verify that execute_due_savings_schedules cannot credit a goal
// more than once for the same due window, regardless of how many times the
// function is invoked at the same ledger timestamp.
// ============================================================================

/// Calling execute_due_savings_schedules twice at the same ledger timestamp
/// for a one-shot (non-recurring) schedule must credit the goal exactly once.
///
/// Security: a one-shot schedule is deactivated (`active = false`) after the
/// first execution.  The second call must be a no-op and must not alter the
/// goal balance.
#[test]
fn test_execute_oneshot_schedule_idempotent() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Emergency"), &5000, &9999);
    // One-shot schedule: interval = 0
    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &500, &3000, &0);

    // Advance time past the due date; both calls share the same timestamp.
    set_ledger_time(&env, 2, 3500);

    let first = client.execute_due_savings_schedules();
    let second = client.execute_due_savings_schedules();

    // First call must have executed the schedule.
    assert_eq!(first.len(), 1, "First call should execute one schedule");
    assert_eq!(first.get(0).unwrap(), schedule_id);

    // Second call must be a no-op (schedule is inactive after first execution).
    assert_eq!(second.len(), 0, "Second call must not re-execute the schedule");

    // Goal balance must reflect exactly one credit.
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 500, "Goal must be credited exactly once");

    // Schedule must be inactive.
    let schedule = client.get_savings_schedule(&schedule_id).unwrap();
    assert!(!schedule.active, "One-shot schedule must be inactive after execution");
}

/// Calling execute_due_savings_schedules twice at the same ledger timestamp
/// for a recurring schedule must credit the goal exactly once per due window.
///
/// Security: after the first execution `next_due` is advanced past
/// `current_time`, so the second call sees `next_due > current_time` and the
/// idempotency guard (`last_executed >= next_due_original`) both independently
/// prevent re-execution.  This test confirms neither protection is bypassed.
#[test]
fn test_execute_recurring_schedule_idempotent() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Vacation"), &10000, &99999);
    // Recurring schedule with a 1-day interval.
    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &200, &3000, &86400);

    set_ledger_time(&env, 2, 3500);

    let first = client.execute_due_savings_schedules();
    let second = client.execute_due_savings_schedules();

    // First call must execute once.
    assert_eq!(first.len(), 1, "First call should execute one schedule");
    assert_eq!(first.get(0).unwrap(), schedule_id);

    // Second call must be a no-op.
    assert_eq!(second.len(), 0, "Second call must not re-execute the schedule");

    // Goal balance must reflect exactly one credit.
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 200, "Goal must be credited exactly once");

    // Schedule must remain active with next_due advanced past current_time.
    let schedule = client.get_savings_schedule(&schedule_id).unwrap();
    assert!(schedule.active, "Recurring schedule must stay active");
    assert!(
        schedule.next_due > 3500,
        "next_due must be advanced past current_time after execution"
    );
    // last_executed must record when the schedule ran.
    assert_eq!(
        schedule.last_executed,
        Some(3500),
        "last_executed must be set to the execution timestamp"
    );
}

/// Executing a schedule and then calling execute again at a later timestamp
/// (within the next interval) must produce exactly one additional credit.
///
/// This confirms that after `next_due` is advanced the schedule correctly
/// fires again in the following window and does not double-fire.
#[test]
fn test_execute_recurring_fires_again_next_window() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Pension"), &10000, &99999);
    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &300, &3000, &1000);

    // First window: execute at t=3500 (past due t=3000)
    set_ledger_time(&env, 2, 3500);
    let first = client.execute_due_savings_schedules();
    assert_eq!(first.len(), 1);

    // Goal has one credit.
    let goal_after_first = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal_after_first.current_amount, 300);

    // Second window: execute at t=4500 (past advanced next_due t=4000)
    set_ledger_time(&env, 3, 4500);
    let second = client.execute_due_savings_schedules();
    assert_eq!(second.len(), 1, "Second window must execute once");
    assert_eq!(second.get(0).unwrap(), schedule_id);

    // Goal has two credits (not three or more).
    let goal_after_second = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal_after_second.current_amount, 600, "Goal must have exactly two credits");
}

/// Verifies that `last_executed` is always set to the ledger timestamp at the
/// moment of execution, not to `next_due` or any other derived value.
///
/// This is required for the idempotency guard (`last_executed >= next_due`) to
/// function correctly when `current_time > next_due` (i.e. the execution was
/// late).
#[test]
fn test_last_executed_set_to_current_time() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Housing"), &10000, &99999);
    // Due at 3000, but we execute late at 5000.
    let schedule_id = client.create_savings_schedule(&owner, &goal_id, &100, &3000, &0);

    set_ledger_time(&env, 2, 5000);
    client.execute_due_savings_schedules();

    let schedule = client.get_savings_schedule(&schedule_id).unwrap();
    assert_eq!(
        schedule.last_executed,
        Some(5000),
        "last_executed must equal current_time (5000), not next_due (3000)"
    );
}

// ============================================================================
// End-to-end migration compatibility tests — savings_goals ↔ data_migration
//
// These tests exercise the full export ↔ import pipeline across both
// packages: the Soroban contract (savings_goals) and the off-chain migration
// utilities (data_migration). All four format paths are covered.
//
// Approach:
//   1. Use the Soroban test env to create real on-chain goal state.
//   2. Call `export_snapshot()` to get a `GoalsExportSnapshot`.
//   3. Convert to `data_migration::SavingsGoalsExport` (field mapping).
//   4. Use `data_migration` helpers to serialize, deserialize, and validate.
//   5. Assert field fidelity and security invariants.
//
// Security invariants validated:
//   - Checksum integrity is preserved across all format paths.
//   - Tampered checksums are rejected by `validate_for_import`.
//   - Incompatible schema versions are rejected.
//   - `locked` and `unlock_date` flags are faithfully exported.
// ============================================================================
#[cfg(test)]
mod migration_e2e_tests {
    use super::*;
    use data_migration::{
        build_savings_snapshot, export_to_binary, export_to_csv, export_to_encrypted_payload,
        export_to_json, import_from_binary, import_from_encrypted_payload, import_from_json,
        import_goals_from_csv, ExportFormat, MigrationError, SavingsGoalExport,
        SavingsGoalsExport, SnapshotPayload, SCHEMA_VERSION,
    };
    use soroban_sdk::{testutils::Address as AddressTrait, Address, Env};
    extern crate alloc;
    use alloc::vec::Vec as StdVec;

    // -------------------------------------------------------------------------
    // Helper: convert an on-chain GoalsExportSnapshot into a data_migration export.
    // -------------------------------------------------------------------------

    /// Convert a `GoalsExportSnapshot` (from the contract) into a
    /// `data_migration::SavingsGoalsExport` (for off-chain processing).
    ///
    /// The `owner` field in `SavingsGoal` is a `soroban_sdk::Address`; we
    /// convert it to a hex string using its debug representation so the
    /// off-chain struct can store it as a plain `String`.
    fn to_migration_export(snapshot: &GoalsExportSnapshot, _env: &Env) -> SavingsGoalsExport {
        let mut goals: StdVec<SavingsGoalExport> = StdVec::new();
        for i in 0..snapshot.goals.len() {
            if let Some(g) = snapshot.goals.get(i) {
                // Convert soroban_sdk::String to alloc String via byte buffer.
                let name_str: alloc::string::String = {
                    let len = g.name.len() as usize;
                    let mut buf = alloc::vec![0u8; len];
                    g.name.copy_into_slice(&mut buf);
                    alloc::string::String::from_utf8_lossy(&buf).into_owned()
                };
                goals.push(SavingsGoalExport {
                    id: g.id,
                    owner: alloc::format!("{:?}", g.owner),
                    name: name_str,
                    // SavingsGoal uses i128; data_migration stores i64.
                    // Test amounts are small so the cast is safe.
                    target_amount: g.target_amount as i64,
                    current_amount: g.current_amount as i64,
                    target_date: g.target_date,
                    locked: g.locked,
                });
            }
        }
        SavingsGoalsExport {
            next_id: snapshot.next_id,
            goals,
        }
    }

    // -------------------------------------------------------------------------
    // JSON format
    // -------------------------------------------------------------------------

    /// E2E: export on-chain goals → data_migration JSON bytes → import → verify fields.
    ///
    /// Tests the complete pipeline: contract state → `export_snapshot` →
    /// `SavingsGoalsExport` → `build_savings_snapshot` (JSON) →
    /// `export_to_json` → `import_from_json` → field assertions.
    #[test]
    fn test_e2e_contract_export_import_json_roundtrip() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.init();
        let goal_id = client.create_goal(
            &owner,
            &String::from_str(&env, "Vacation"),
            &10_000i128,
            &2_000_000_000u64,
        );
        client.add_to_goal(&owner, &goal_id, &3_500i128);

        // Export on-chain snapshot.
        let snapshot = client.export_snapshot(&owner);
        assert_eq!(snapshot.version, 1);
        assert_eq!(snapshot.goals.len(), 1);

        // Convert and build migration snapshot.
        let migration_export = to_migration_export(&snapshot, &env);
        assert_eq!(migration_export.next_id, 1);
        assert_eq!(migration_export.goals.len(), 1);
        let mig_goal = &migration_export.goals[0];
        assert_eq!(mig_goal.id, 1);
        assert_eq!(mig_goal.target_amount, 10_000);
        assert_eq!(mig_goal.current_amount, 3_500);
        assert_eq!(mig_goal.target_date, 2_000_000_000);

        let mig_snapshot = build_savings_snapshot(migration_export, ExportFormat::Json);
        assert!(mig_snapshot.verify_checksum());

        // Serialize to JSON and reimport.
        let bytes = export_to_json(&mig_snapshot).unwrap();
        let loaded = import_from_json(&bytes).unwrap();
        assert_eq!(loaded.header.version, SCHEMA_VERSION);
        assert!(loaded.verify_checksum());

        if let SnapshotPayload::SavingsGoals(ref g) = loaded.payload {
            assert_eq!(g.goals.len(), 1);
            assert_eq!(g.goals[0].target_amount, 10_000);
            assert_eq!(g.goals[0].current_amount, 3_500);
            assert_eq!(g.goals[0].target_date, 2_000_000_000);
        } else {
            panic!("Expected SavingsGoals payload");
        }
    }

    // -------------------------------------------------------------------------
    // Binary format
    // -------------------------------------------------------------------------

    /// E2E: contract export → binary serialization → import → checksum verified.
    #[test]
    fn test_e2e_contract_export_import_binary_roundtrip() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.init();
        let goal_id = client.create_goal(
            &owner,
            &String::from_str(&env, "Emergency"),
            &20_000i128,
            &1_900_000_000u64,
        );
        client.add_to_goal(&owner, &goal_id, &5_000i128);

        let snapshot = client.export_snapshot(&owner);
        let migration_export = to_migration_export(&snapshot, &env);

        let mig_snapshot = build_savings_snapshot(migration_export, ExportFormat::Binary);
        assert!(mig_snapshot.verify_checksum());

        let bytes = export_to_binary(&mig_snapshot).unwrap();
        assert!(!bytes.is_empty());

        let loaded = import_from_binary(&bytes).unwrap();
        assert_eq!(loaded.header.version, SCHEMA_VERSION);
        assert_eq!(loaded.header.format, "binary");
        assert!(loaded.verify_checksum());

        if let SnapshotPayload::SavingsGoals(ref g) = loaded.payload {
            assert_eq!(g.goals[0].target_amount, 20_000);
            assert_eq!(g.goals[0].current_amount, 5_000);
        } else {
            panic!("Expected SavingsGoals payload");
        }
    }

    // -------------------------------------------------------------------------
    // CSV format
    // -------------------------------------------------------------------------

    /// E2E: multiple contract goals → CSV export → import → all records preserved.
    #[test]
    fn test_e2e_contract_export_import_csv_roundtrip() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.init();
        let id1 = client.create_goal(
            &owner,
            &String::from_str(&env, "Trip"),
            &8_000i128,
            &2_000_000_000u64,
        );
        let id2 = client.create_goal(
            &owner,
            &String::from_str(&env, "Gadget"),
            &3_000i128,
            &2_000_000_000u64,
        );
        client.add_to_goal(&owner, &id1, &2_000i128);
        client.add_to_goal(&owner, &id2, &1_500i128);

        let snapshot = client.export_snapshot(&owner);
        assert_eq!(snapshot.goals.len(), 2);

        let migration_export = to_migration_export(&snapshot, &env);
        let csv_bytes = export_to_csv(&migration_export).unwrap();
        assert!(!csv_bytes.is_empty());

        let goals = import_goals_from_csv(&csv_bytes).unwrap();
        assert_eq!(goals.len(), 2, "both goals must survive CSV roundtrip");

        // Verify amounts are preserved.
        let g1 = goals.iter().find(|g| g.id == 1).expect("goal 1 must be present");
        let g2 = goals.iter().find(|g| g.id == 2).expect("goal 2 must be present");
        assert_eq!(g1.target_amount, 8_000);
        assert_eq!(g1.current_amount, 2_000);
        assert_eq!(g2.target_amount, 3_000);
        assert_eq!(g2.current_amount, 1_500);
    }

    // -------------------------------------------------------------------------
    // Encrypted format
    // -------------------------------------------------------------------------

    /// E2E: contract export → JSON bytes → base64 wrap → decode → re-import.
    ///
    /// Simulates the encrypted-channel path: caller serialises to JSON, wraps
    /// in base64 (as would an encryption layer), transmits, then decodes
    /// and re-imports.
    #[test]
    fn test_e2e_contract_export_import_encrypted_roundtrip() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.init();
        let goal_id = client.create_goal(
            &owner,
            &String::from_str(&env, "House"),
            &500_000i128,
            &2_100_000_000u64,
        );
        client.add_to_goal(&owner, &goal_id, &100_000i128);

        let snapshot = client.export_snapshot(&owner);
        let migration_export = to_migration_export(&snapshot, &env);

        // Build and serialize to JSON ("plaintext" before encryption).
        let mig_snapshot = build_savings_snapshot(migration_export, ExportFormat::Encrypted);
        assert!(mig_snapshot.verify_checksum());
        let plain_bytes = export_to_json(&mig_snapshot).unwrap();

        // Encrypt (base64 encode).
        let encoded = export_to_encrypted_payload(&plain_bytes);
        assert!(!encoded.is_empty());

        // Decrypt (base64 decode).
        let decoded = import_from_encrypted_payload(&encoded).unwrap();
        assert_eq!(decoded, plain_bytes);

        // Re-import and validate.
        let loaded = import_from_json(&decoded).unwrap();
        assert!(loaded.verify_checksum());
        if let SnapshotPayload::SavingsGoals(ref g) = loaded.payload {
            assert_eq!(g.goals[0].target_amount, 500_000);
            assert_eq!(g.goals[0].current_amount, 100_000);
        } else {
            panic!("Expected SavingsGoals payload");
        }
    }

    // -------------------------------------------------------------------------
    // Security: tampered checksum rejected
    // -------------------------------------------------------------------------

    /// E2E: mutating the header checksum after export must fail import validation.
    ///
    /// Security invariant: any post-export mutation is detected by the SHA-256
    /// checksum and causes `validate_for_import` to return `ChecksumMismatch`.
    #[test]
    fn test_e2e_tampered_checksum_fails_import() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.init();
        client.create_goal(
            &owner,
            &String::from_str(&env, "Security Test"),
            &1_000i128,
            &2_000_000_000u64,
        );

        let snapshot = client.export_snapshot(&owner);
        let migration_export = to_migration_export(&snapshot, &env);
        let mut mig_snapshot = build_savings_snapshot(migration_export, ExportFormat::Json);

        assert!(mig_snapshot.verify_checksum(), "fresh snapshot must be valid");

        // Tamper.
        mig_snapshot.header.checksum = "00000000000000000000000000000000".into();

        assert!(!mig_snapshot.verify_checksum());
        assert_eq!(
            mig_snapshot.validate_for_import(),
            Err(MigrationError::ChecksumMismatch)
        );
    }

    // -------------------------------------------------------------------------
    // Security: incompatible version rejected
    // -------------------------------------------------------------------------

    /// E2E: setting schema version below `MIN_SUPPORTED_VERSION` must cause
    /// `validate_for_import` to return `IncompatibleVersion`.
    #[test]
    fn test_e2e_incompatible_version_fails_import() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.init();
        client.create_goal(
            &owner,
            &String::from_str(&env, "Version Test"),
            &500i128,
            &2_000_000_000u64,
        );

        let snapshot = client.export_snapshot(&owner);
        let migration_export = to_migration_export(&snapshot, &env);
        let mut mig_snapshot = build_savings_snapshot(migration_export, ExportFormat::Json);

        mig_snapshot.header.version = 0; // unsupported

        assert!(matches!(
            mig_snapshot.validate_for_import(),
            Err(MigrationError::IncompatibleVersion { found: 0, .. })
        ));
    }

    // -------------------------------------------------------------------------
    // Edge case: empty contract state
    // -------------------------------------------------------------------------

    /// E2E: exporting a contract with zero goals must produce a valid empty snapshot
    /// that survives the JSON roundtrip.
    #[test]
    fn test_e2e_empty_contract_export_json_roundtrip() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.init();

        // Export with no goals created.
        let snapshot = client.export_snapshot(&owner);
        assert_eq!(snapshot.goals.len(), 0);

        let migration_export = to_migration_export(&snapshot, &env);
        assert_eq!(migration_export.goals.len(), 0);

        let mig_snapshot = build_savings_snapshot(migration_export, ExportFormat::Json);
        assert!(mig_snapshot.verify_checksum());

        let bytes = export_to_json(&mig_snapshot).unwrap();
        let loaded = import_from_json(&bytes).unwrap();
        assert!(loaded.verify_checksum());

        if let SnapshotPayload::SavingsGoals(ref g) = loaded.payload {
            assert_eq!(g.goals.len(), 0);
        } else {
            panic!("Expected SavingsGoals payload");
        }
    }

    // -------------------------------------------------------------------------
    // Edge case: locked goal preserved through migration
    // -------------------------------------------------------------------------

    /// E2E: a goal with `locked: true` must have its locked flag faithfully
    /// preserved through the full export → JSON → import pipeline.
    ///
    /// Validates that the `locked` field survives the contract-to-migration
    /// struct conversion and the JSON serialization layer.
    #[test]
    fn test_e2e_locked_goal_preserved_through_migration() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.init();
        let goal_id = client.create_goal(
            &owner,
            &String::from_str(&env, "Locked Goal"),
            &10_000i128,
            &2_000_000_000u64,
        );
        client.add_to_goal(&owner, &goal_id, &5_000i128);
        // Goal is created locked by default; verify it is still locked.
        let goal = client.get_goal(&goal_id).unwrap();
        assert!(goal.locked, "goal must be locked after create_goal");

        // Export and convert.
        let snapshot = client.export_snapshot(&owner);
        let migration_export = to_migration_export(&snapshot, &env);
        assert!(
            migration_export.goals[0].locked,
            "locked flag must survive contract → migration conversion"
        );

        // Roundtrip through JSON.
        let mig_snapshot = build_savings_snapshot(migration_export, ExportFormat::Json);
        let bytes = export_to_json(&mig_snapshot).unwrap();
        let loaded = import_from_json(&bytes).unwrap();

        if let SnapshotPayload::SavingsGoals(ref g) = loaded.payload {
            assert!(
                g.goals[0].locked,
                "locked flag must be true after JSON roundtrip"
            );
        } else {
            panic!("Expected SavingsGoals payload");
        }
    }

    // -------------------------------------------------------------------------
    // Determinism: same state → same checksum
    // -------------------------------------------------------------------------

    /// E2E: exporting the same contract state twice and building migration
    /// snapshots from both must yield identical checksums.
    #[test]
    fn test_e2e_snapshot_checksum_is_stable() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.init();
        let goal_id = client.create_goal(
            &owner,
            &String::from_str(&env, "Stable"),
            &7_000i128,
            &2_000_000_000u64,
        );
        client.add_to_goal(&owner, &goal_id, &2_000i128);

        // Export twice.
        let snap_a = client.export_snapshot(&owner);
        let snap_b = client.export_snapshot(&owner);

        let mig_a = build_savings_snapshot(to_migration_export(&snap_a, &env), ExportFormat::Json);
        let mig_b = build_savings_snapshot(to_migration_export(&snap_b, &env), ExportFormat::Json);

        assert_eq!(
            mig_a.header.checksum, mig_b.header.checksum,
            "same contract state must produce deterministic checksums"
        );
    }

    // -------------------------------------------------------------------------
    // Multi-goal, multi-owner export
    // -------------------------------------------------------------------------

    /// E2E: export goals from two separate contract owners, then roundtrip via
    /// JSON — all goals and owner IDs must be preserved.
    #[test]
    fn test_e2e_multi_owner_export_import_json_roundtrip() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);

        client.init();

        // Create goals for owner A.
        let a1 = client.create_goal(
            &owner_a,
            &String::from_str(&env, "A Car"),
            &30_000i128,
            &2_000_000_000u64,
        );
        client.add_to_goal(&owner_a, &a1, &10_000i128);

        // Create goals for owner B.
        let b1 = client.create_goal(
            &owner_b,
            &String::from_str(&env, "B Education"),
            &50_000i128,
            &2_000_000_000u64,
        );
        client.add_to_goal(&owner_b, &b1, &15_000i128);

        // Export full contract state via owner A's call.
        // `export_snapshot` returns ALL goals (not filtered by caller).
        let snapshot = client.export_snapshot(&owner_a);
        assert_eq!(snapshot.goals.len(), 2, "both owners' goals must appear in snapshot");

        let migration_export = to_migration_export(&snapshot, &env);
        let mig_snapshot = build_savings_snapshot(migration_export, ExportFormat::Json);
        assert!(mig_snapshot.verify_checksum());

        let bytes = export_to_json(&mig_snapshot).unwrap();
        let loaded = import_from_json(&bytes).unwrap();
        assert!(loaded.verify_checksum());

        if let SnapshotPayload::SavingsGoals(ref g) = loaded.payload {
            assert_eq!(g.goals.len(), 2);

            let ga = g.goals.iter().find(|g| g.id == 1).expect("goal 1");
            let gb = g.goals.iter().find(|g| g.id == 2).expect("goal 2");

            assert_eq!(ga.target_amount, 30_000);
            assert_eq!(ga.current_amount, 10_000);
            assert_eq!(gb.target_amount, 50_000);
            assert_eq!(gb.current_amount, 15_000);
        } else {
            panic!("Expected SavingsGoals payload");
        }
    }
}
