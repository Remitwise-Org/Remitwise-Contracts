#![cfg(test)]

//! Stress tests for arithmetic operations with very large i128 values in remittance_split
//!
//! These tests verify that the remittance_split contract handles extreme values correctly:
//! - Values near i128::MAX/2 to test multiplication and division operations
//! - Proper overflow detection using checked arithmetic
//! - No unexpected panics or wrap-around behavior
//!
//! ## Documented Limitations
//! - calculate_split uses checked_mul and checked_div to prevent overflow
//! - Maximum safe amount depends on split percentages (multiplication can overflow)
//! - Overflow returns RemittanceSplitError::Overflow rather than panicking
//! - For 100% total split, max safe value is approximately i128::MAX / 100

use remittance_split::{RemittanceSplit, RemittanceSplitClient, RemittanceSplitError};
use soroban_sdk::testutils::Address as AddressTrait;
use soroban_sdk::{Env, String};

#[test]
fn test_calculate_split_with_large_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Initialize with standard split: 50% spending, 30% savings, 15% bills, 5% insurance
    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    // Test with i128::MAX / 200 to ensure multiplication by percentages doesn't overflow
    let large_amount = i128::MAX / 200;

    let result = client.calculate_split(&large_amount);
    assert!(result.is_ok());

    let amounts = result.unwrap();
    assert_eq!(amounts.len(), 4);

    // Verify the split adds up correctly
    let total: i128 = amounts.iter().sum();
    assert_eq!(total, large_amount);
}

#[test]
fn test_calculate_split_near_max_safe_value() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    // Maximum safe value for multiplication by 100 (largest percentage)
    let max_safe = i128::MAX / 100 - 1;

    let result = client.calculate_split(&max_safe);
    assert!(result.is_ok());

    let amounts = result.unwrap();
    let total: i128 = amounts.iter().sum();

    // Total should equal input (within rounding)
    assert!((total - max_safe).abs() < 4); // Allow small rounding difference
}

#[test]
fn test_calculate_split_overflow_detection() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    // Value that will overflow when multiplied by percentage
    let overflow_amount = i128::MAX / 50; // Will overflow when multiplied by 50

    let result = client.try_calculate_split(&overflow_amount);

    // Should return Overflow error, not panic
    assert_eq!(result, Err(Ok(RemittanceSplitError::Overflow)));
}

#[test]
fn test_calculate_split_with_minimal_percentages() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Use minimal percentages to allow larger amounts
    client.initialize_split(&owner, &0, &1, &1, &1, &97);

    // With 1% multiplier, we can handle much larger values
    let large_amount = i128::MAX / 150;

    let result = client.calculate_split(&large_amount);
    assert!(result.is_ok());

    let amounts = result.unwrap();
    let total: i128 = amounts.iter().sum();
    assert_eq!(total, large_amount);
}

#[test]
fn test_get_split_allocations_with_large_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let large_amount = i128::MAX / 200;

    let result = client.get_split_allocations(&large_amount);
    assert!(result.is_ok());

    let allocations = result.unwrap();
    assert_eq!(allocations.len(), 4);

    // Verify each allocation has correct category and reasonable amount
    let total: i128 = allocations.iter().map(|a| a.amount).sum();
    assert_eq!(total, large_amount);
}

#[test]
fn test_multiple_splits_with_large_amounts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let large_amount = i128::MAX / 300;

    // Perform multiple splits to ensure no state corruption
    for _ in 0..5 {
        let result = client.calculate_split(&large_amount);
        assert!(result.is_ok());

        let amounts = result.unwrap();
        let total: i128 = amounts.iter().sum();
        assert_eq!(total, large_amount);
    }
}

#[test]
fn test_edge_case_i128_max_divided_by_100() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    // Exact edge case: i128::MAX / 100
    let edge_amount = i128::MAX / 100;

    let result = client.calculate_split(&edge_amount);
    assert!(result.is_ok());

    let amounts = result.unwrap();
    assert_eq!(amounts.len(), 4);
}

#[test]
fn test_split_with_100_percent_to_one_category() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // 100% to spending, 0% to others
    client.initialize_split(&owner, &0, &100, &0, &0, &0);

    let large_amount = i128::MAX / 150;

    let result = client.calculate_split(&large_amount);
    assert!(result.is_ok());

    let amounts = result.unwrap();
    // First amount should be the full amount
    assert_eq!(amounts.get(0).unwrap(), large_amount);
    // Others should be 0
    assert_eq!(amounts.get(1).unwrap(), 0);
    assert_eq!(amounts.get(2).unwrap(), 0);
    assert_eq!(amounts.get(3).unwrap(), 0);
}

#[test]
fn test_rounding_behavior_with_large_amounts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Use percentages that don't divide evenly
    client.initialize_split(&owner, &0, &33, &33, &33, &1);

    let large_amount = i128::MAX / 200;

    let result = client.calculate_split(&large_amount);
    assert!(result.is_ok());

    let amounts = result.unwrap();
    let total: i128 = amounts.iter().sum();

    // Due to rounding, total should equal input
    assert_eq!(total, large_amount);
}

#[test]
fn test_sequential_large_calculations() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    // Test with progressively larger amounts
    let amounts_to_test = vec![
        i128::MAX / 1000,
        i128::MAX / 500,
        i128::MAX / 200,
        i128::MAX / 150,
        i128::MAX / 100,
    ];

    for amount in amounts_to_test {
        let result = client.calculate_split(&amount);
        assert!(result.is_ok(), "Failed for amount: {}", amount);

        let splits = result.unwrap();
        let total: i128 = splits.iter().sum();
        assert_eq!(total, amount);
    }
}

#[test]
fn test_checked_arithmetic_prevents_silent_overflow() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    // Test values that would overflow with unchecked arithmetic
    let dangerous_amounts = vec![
        i128::MAX / 40, // Will overflow when multiplied by 50
        i128::MAX / 30, // Will overflow when multiplied by 50
        i128::MAX,      // Will definitely overflow
    ];

    for amount in dangerous_amounts {
        let result = client.try_calculate_split(&amount);
        // Should return error, not panic or wrap around
        assert!(
            result.is_err(),
            "Should have detected overflow for amount: {}",
            amount
        );
    }
}

#[test]
fn test_insurance_remainder_calculation_with_large_values() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Insurance gets the remainder after other allocations
    client.initialize_split(&owner, &0, &40, &30, &20, &10);

    let large_amount = i128::MAX / 200;

    let result = client.calculate_split(&large_amount);
    assert!(result.is_ok());

    let amounts = result.unwrap();

    // Verify insurance (last element) is calculated correctly as remainder
    let spending = amounts.get(0).unwrap();
    let savings = amounts.get(1).unwrap();
    let bills = amounts.get(2).unwrap();
    let insurance = amounts.get(3).unwrap();

    assert_eq!(spending + savings + bills + insurance, large_amount);
}
