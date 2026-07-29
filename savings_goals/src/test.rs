use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

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
#[should_panic] // It will panic because the goal doesn't exist
fn test_add_to_non_existent_goal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    client.add_to_goal(&user, &99, &500);
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
#[should_panic]
fn test_zero_amount_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    client.create_goal(&user, &String::from_str(&env, "Fail"), &0, &2000000000);
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
