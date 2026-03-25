#![cfg(test)]

//! Stress tests for arithmetic operations with very large i128 values in remittance_split.

use remittance_split::{RemittanceSplit, RemittanceSplitClient};
use soroban_sdk::testutils::Address as AddressTrait;
use soroban_sdk::{Address, Env};

fn dummy_token(env: &Env) -> Address {
    Address::generate(env)
}

fn init(client: &RemittanceSplitClient, env: &Env, owner: &Address, s: u32, g: u32, b: u32, i: u32) {
    let token = dummy_token(env);
    client.initialize_split(owner, &0, &token, &s, &g, &b, &i);
}

#[test]
fn test_calculate_split_with_large_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 50, 30, 15, 5);

    let large_amount = i128::MAX / 200;
    let result = client.try_calculate_split(&large_amount);
    assert!(result.is_ok());
    let amounts = result.unwrap().unwrap();
    assert_eq!(amounts.len(), 4);
    let total: i128 = amounts.iter().sum();
    assert_eq!(total, large_amount);
}

#[test]
fn test_calculate_split_near_max_safe_value() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 50, 30, 15, 5);

    let max_safe = i128::MAX / 100 - 1;
    let result = client.try_calculate_split(&max_safe);
    assert!(result.is_ok());
    let amounts = result.unwrap().unwrap();
    let total: i128 = amounts.iter().sum();
    assert!((total - max_safe).abs() < 4);
}

#[test]
fn test_calculate_split_with_minimal_percentages() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 1, 1, 1, 97);

    let large_amount = i128::MAX / 150;
    let result = client.try_calculate_split(&large_amount);
    assert!(result.is_ok());
    let amounts = result.unwrap().unwrap();
    let total: i128 = amounts.iter().sum();
    assert_eq!(total, large_amount);
}

#[test]
fn test_get_split_allocations_with_large_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 50, 30, 15, 5);

    let large_amount = i128::MAX / 200;
    let result = client.try_get_split_allocations(&large_amount);
    assert!(result.is_ok());
    let allocations = result.unwrap().unwrap();
    assert_eq!(allocations.len(), 4);
    let total: i128 = allocations.iter().map(|a| a.amount).sum();
    assert_eq!(total, large_amount);
}

#[test]
fn test_multiple_splits_with_large_amounts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 50, 30, 15, 5);

    let large_amount = i128::MAX / 300;
    for _ in 0..5 {
        let result = client.try_calculate_split(&large_amount);
        assert!(result.is_ok());
        let amounts = result.unwrap().unwrap();
        let total: i128 = amounts.iter().sum();
        assert_eq!(total, large_amount);
    }
}

#[test]
fn test_edge_case_i128_max_divided_by_100() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 50, 30, 15, 5);

    let edge_amount = i128::MAX / 100;
    let result = client.try_calculate_split(&edge_amount);
    assert!(result.is_ok());
    let amounts = result.unwrap().unwrap();
    assert_eq!(amounts.len(), 4);
}

#[test]
fn test_split_with_100_percent_to_one_category() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 100, 0, 0, 0);

    let large_amount = i128::MAX / 150;
    let result = client.try_calculate_split(&large_amount);
    assert!(result.is_ok());
    let amounts = result.unwrap().unwrap();
    assert_eq!(amounts.get(0).unwrap(), large_amount);
    assert_eq!(amounts.get(1).unwrap(), 0);
    assert_eq!(amounts.get(2).unwrap(), 0);
    assert_eq!(amounts.get(3).unwrap(), 0);
}

#[test]
fn test_rounding_behavior_with_large_amounts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 33, 33, 33, 1);

    let large_amount = i128::MAX / 200;
    let result = client.try_calculate_split(&large_amount);
    assert!(result.is_ok());
    let amounts = result.unwrap().unwrap();
    let total: i128 = amounts.iter().sum();
    assert_eq!(total, large_amount);
}

#[test]
fn test_sequential_large_calculations() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 50, 30, 15, 5);

    for amount in &[i128::MAX / 1000, i128::MAX / 500, i128::MAX / 200, i128::MAX / 150, i128::MAX / 100] {
        let result = client.try_calculate_split(amount);
        assert!(result.is_ok(), "Failed for amount: {}", amount);
        let splits = result.unwrap().unwrap();
        let total: i128 = splits.iter().sum();
        assert_eq!(total, *amount, "Failed for amount: {}", amount);
    }
}

#[test]
fn test_checked_arithmetic_prevents_silent_overflow() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 50, 30, 15, 5);

    for amount in &[i128::MAX / 40, i128::MAX / 30, i128::MAX] {
        let result = client.try_calculate_split(amount);
        assert!(result.is_err(), "Should have detected overflow for amount: {}", amount);
    }
}

#[test]
fn test_insurance_remainder_calculation_with_large_values() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 40, 30, 20, 10);

    let large_amount = i128::MAX / 200;
    let result = client.try_calculate_split(&large_amount);
    assert!(result.is_ok());
    let amounts = result.unwrap().unwrap();
    let total: i128 = amounts.iter().sum();
    assert_eq!(total, large_amount);
}
