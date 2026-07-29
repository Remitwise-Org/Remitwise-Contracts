#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as AddressTrait, Events, Ledger, LedgerInfo},
    Address, Env, IntoVal, String, Symbol, TryFromVal, Vec as SorobanVec,
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

    let id1 = client.create_goal(&user, &name1, &1000, &1735689600, &false);
    let id2 = client.create_goal(&user, &name2, &2000, &1735689600, &false);

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
    let id = client.create_goal(&user, &name, &1000, &past_target_date, &false);

    assert_eq!(id, 1);
    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.target_date, past_target_date);
}

// ============================================================================
// init() idempotency and NEXT_ID behavior
//
// init() bootstraps storage (NEXT_ID and GOALS) only when keys are missing.

#[test]
fn test_create_goal_empty_name_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    client.init();

    let name = String::from_str(&env, "");
    let res = client.try_create_goal(&user, &name, &1000, &1735689600, &false);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().unwrap(), SavingsGoalError::InvalidGoalName);
}

#[test]
fn test_create_goal_max_len_name_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    client.init();

    // 32 chars
    let name = String::from_str(&env, "Test Goal Name Exactly 32 Chars.");
    let id = client.create_goal(&user, &name, &1000, &1735689600, &false);
    assert_eq!(id, 1);
}

#[test]
fn test_create_goal_over_max_len_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    client.init();

    // 33 bytes (exceeds MAX_GOAL_NAME_LEN_BYTES = 32)
    let name = String::from_str(&env, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    let res = client.try_create_goal(&user, &name, &1000, &1735689600, &false);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().unwrap(), SavingsGoalError::InvalidGoalName);
}

#[test]
fn test_create_goal_control_char_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    client.init();

    // Contains newline \n
    let name = String::from_str(&env, "Goal\nName");
    let res = client.try_create_goal(&user, &name, &1000, &1735689600, &false);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().unwrap(), SavingsGoalError::InvalidGoalName);
}
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

    let goal_id_1 = client.create_goal(&owner_a, &name1, &target1, &target_date1, &false);
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
    let goal_id_2 = client.create_goal(&owner_a, &name2, &10000i128, &target_date1, &false);
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
            &false,
        ),
        client.create_goal(
            &owner,
            &String::from_str(&env, "G2"),
            &2000i128,
            &2000000000u64,
            &false,
        ),
        client.create_goal(
            &owner,
            &String::from_str(&env, "G3"),
            &3000i128,
            &2000000000u64,
            &false,
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
    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Save"),
        &1000,
        &2000000000,
        &false,
    );

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
    let id = client.create_goal(&user, &name, &5000, &2000000000, &false);

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
    client.create_goal(
        &user,
        &String::from_str(&env, "A"),
        &100,
        &2000000000,
        &false,
    );
    client.create_goal(
        &user,
        &String::from_str(&env, "B"),
        &200,
        &2000000000,
        &false,
    );

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
    let id = client.create_goal(&user, &name, &target, &2000000000, &false);

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
    let safe_cap = i128::MAX / 2;
    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Max"),
        &safe_cap,
        &2000000000,
        &false,
    );

    client.add_to_goal(&user, &id, &(safe_cap - 100));
    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.current_amount, safe_cap - 100);
}

#[test]
fn test_zero_amount_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let res = client.try_create_goal(
        &user,
        &String::from_str(&env, "Fail"),
        &0,
        &2000000000,
        &false,
    );
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
    let id1 = client.create_goal(
        &user,
        &String::from_str(&env, "G1"),
        &1000,
        &2000000000,
        &false,
    );
    let id2 = client.create_goal(
        &user,
        &String::from_str(&env, "G2"),
        &2000,
        &2000000000,
        &false,
    );

    client.add_to_goal(&user, &id1, &500);
    client.add_to_goal(&user, &id2, &1500);

    let g1 = client.get_goal(&id1).unwrap();
    let g2 = client.get_goal(&id2).unwrap();

    assert_eq!(g1.current_amount, 500);
    assert_eq!(g2.current_amount, 1500);
}

#[test]
fn test_withdraw_from_goal_succeeds_once_unlocked() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Trip"), &1000, &2000000000);
    client.add_to_goal(&user, &id, &500);
    client.unlock_goal(&user, &id);

    let remaining = client.withdraw_from_goal(&user, &id, &200);

    assert_eq!(remaining, 300);
    assert_eq!(client.get_goal(&id).unwrap().current_amount, 300);
}

#[test]
fn test_withdraw_from_goal_rejects_non_positive_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Trip"), &1000, &2000000000);
    client.unlock_goal(&user, &id);

    let result = client.try_withdraw_from_goal(&user, &id, &0);

    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_withdraw_from_nonexistent_goal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let result = client.try_withdraw_from_goal(&user, &99, &100);

    assert_eq!(result, Err(Ok(Error::GoalNotFound)));
}

#[test]
fn test_withdraw_from_goal_rejects_non_owner() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&owner, &String::from_str(&env, "Trip"), &1000, &2000000000);
    client.add_to_goal(&owner, &id, &500);
    client.unlock_goal(&owner, &id);

    let result = client.try_withdraw_from_goal(&stranger, &id, &100);

    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_withdraw_from_locked_goal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    // Goals are created locked by default -- never explicitly unlocked here.
    let id = client.create_goal(&user, &String::from_str(&env, "Trip"), &1000, &2000000000);
    client.add_to_goal(&user, &id, &500);

    let result = client.try_withdraw_from_goal(&user, &id, &100);

    assert_eq!(result, Err(Ok(Error::GoalLocked)));
}

#[test]
fn test_withdraw_more_than_balance() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Trip"), &1000, &2000000000);
    client.add_to_goal(&user, &id, &500);
    client.unlock_goal(&user, &id);

    let result = client.try_withdraw_from_goal(&user, &id, &501);

    assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
}
