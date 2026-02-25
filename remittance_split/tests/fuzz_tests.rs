#![cfg(test)]

//! Fuzz/Property-based tests for numeric operations in remittance_split
//!
//! Note: Due to Soroban SDK's no_std environment and custom types, we use a simpler
//! fuzzing approach with handwritten test cases covering edge cases rather than
//! full proptest integration. This is documented per issue #109 requirements.
//!
//! These tests verify critical numeric invariants:
//! - Overflow protection
//! - Rounding behavior
//! - Sum preservation (split amounts always equal total)
//! - Edge cases with extreme values

use remittance_split::{RemittanceSplit, RemittanceSplitClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Test that calculate_split preserves sum across many random inputs
#[test]
fn fuzz_calculate_split_sum_preservation() {
    // Test various amounts and percentage combinations
    let test_cases = vec![
        // (amount, spending%, savings%, bills%, insurance%)
        (1000, 50, 30, 15, 5),
        (1, 25, 25, 25, 25),
        (999, 33, 33, 33, 1),
        (i128::MAX / 100, 25, 25, 25, 25),
        (12345678, 17, 19, 23, 41), // Primes
        (100, 1, 1, 1, 97),
        (999999, 10, 20, 30, 40),
        (7, 40, 30, 20, 10),
        (543210, 70, 10, 10, 10),
        (1000000, 0, 0, 0, 100),
    ];

    for (total_amount, spending_pct, savings_pct, bills_pct, insurance_pct) in test_cases {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Initialize split
        let result = client.try_initialize_split(
            &owner,
            &0,
            &spending_pct,
            &savings_pct,
            &bills_pct,
            &insurance_pct,
        );

        if result.is_err() {
            continue;
        }

        // Calculate split
        let result = client.try_calculate_split(&total_amount);

        if let Err(_) = result {
            continue; // Skip if calculation fails
        }

        let amounts = client.calculate_split(&total_amount);

        let spending = amounts.get(0).unwrap();
        let savings = amounts.get(1).unwrap();
        let bills = amounts.get(2).unwrap();
        let insurance = amounts.get(3).unwrap();

        // Critical invariant: sum must equal total
        let sum = spending + savings + bills + insurance;
        assert_eq!(
            sum,
            total_amount,
            "Sum mismatch: {} + {} + {} + {} = {} != {} (percentages: {}%, {}%, {}%, {}%)",
            spending,
            savings,
            bills,
            insurance,
            sum,
            total_amount,
            spending_pct,
            savings_pct,
            bills_pct,
            insurance_pct
        );

        // All amounts should be non-negative
        assert!(spending >= 0, "Spending is negative: {}", spending);
        assert!(savings >= 0, "Savings is negative: {}", savings);
        assert!(bills >= 0, "Bills is negative: {}", bills);
        assert!(insurance >= 0, "Insurance is negative: {}", insurance);
    }
}

/// Test edge cases with small amounts
#[test]
fn fuzz_calculate_split_small_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &25, &25, &25, &25);

    // Test amounts 1-100
    for amount in 1..=100 {
        let amounts = client.calculate_split(&amount);

        let spending = amounts.get(0).unwrap();
        let savings = amounts.get(1).unwrap();
        let bills = amounts.get(2).unwrap();
        let insurance = amounts.get(3).unwrap();

        // Verify sum
        let sum = spending + savings + bills + insurance;
        assert_eq!(
            sum, amount,
            "Sum mismatch for amount {}: {} != {}",
            amount, sum, amount
        );

        // Verify no amount exceeds total
        assert!(
            spending <= amount,
            "Spending {} exceeds total {}",
            spending,
            amount
        );
        assert!(
            savings <= amount,
            "Savings {} exceeds total {}",
            savings,
            amount
        );
        assert!(bills <= amount, "Bills {} exceeds total {}", bills, amount);
        assert!(
            insurance <= amount,
            "Insurance {} exceeds total {}",
            insurance,
            amount
        );
    }
}

/// Test with prime percentages that cause rounding
#[test]
fn fuzz_rounding_behavior() {
    let prime_percentages = vec![
        (3, 7, 11, 79),
        (13, 17, 23, 47),
        (19, 23, 29, 29),
        (31, 37, 11, 21),
        (41, 43, 7, 9),
    ];

    for (spending_pct, savings_pct, bills_pct, insurance_pct) in prime_percentages {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.initialize_split(
            &owner,
            &0,
            &spending_pct,
            &savings_pct,
            &bills_pct,
            &insurance_pct,
        );

        // Test various amounts
        for amount in &[100, 1000, 9999, 123456] {
            let amounts = client.calculate_split(amount);

            let spending = amounts.get(0).unwrap();
            let savings = amounts.get(1).unwrap();
            let bills = amounts.get(2).unwrap();
            let insurance = amounts.get(3).unwrap();

            let sum = spending + savings + bills + insurance;
            assert_eq!(
                sum,
                *amount,
                "Rounding error: {} + {} + {} + {} = {} != {} (percentages: {}%, {}%, {}%, {}%)",
                spending,
                savings,
                bills,
                insurance,
                sum,
                amount,
                spending_pct,
                savings_pct,
                bills_pct,
                insurance_pct
            );
        }
    }
}

/// Test that invalid amounts are rejected
#[test]
fn fuzz_invalid_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    // Test invalid amounts
    for amount in &[0, -1, -100, -1000, i128::MIN] {
        let result = client.try_calculate_split(amount);
        assert!(result.is_err(), "Expected error for amount {}", amount);
    }
}

/// Test that invalid percentage sums are rejected
#[test]
fn fuzz_invalid_percentages() {
    let invalid_percentages = vec![
        (50, 50, 10, 0),  // Sum = 110
        (25, 25, 25, 24), // Sum = 99
        (100, 0, 0, 1),   // Sum = 101
        (0, 0, 0, 0),     // Sum = 0
        (30, 30, 30, 30), // Sum = 120
    ];

    for (spending_pct, savings_pct, bills_pct, insurance_pct) in invalid_percentages {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let result = client.try_initialize_split(
            &owner,
            &0,
            &spending_pct,
            &savings_pct,
            &bills_pct,
            &insurance_pct,
        );

        let total = spending_pct + savings_pct + bills_pct + insurance_pct;
        if total != 100 {
            assert!(
                result.is_err(),
                "Expected error for percentages summing to {}",
                total
            );
        }
    }
}

/// Test overflow protection with large amounts
#[test]
fn fuzz_large_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &25, &25, &25, &25);

    // Test large amounts that are safe
    let large_amounts = vec![
        i128::MAX / 1000,
        i128::MAX / 100,
        1_000_000_000_000i128,
        999_999_999_999i128,
    ];

    for amount in large_amounts {
        let result = client.try_calculate_split(&amount);

        // Should either succeed with correct sum, or fail with overflow
        if result.is_ok() {
            let amounts = client.calculate_split(&amount);
            let spending = amounts.get(0).unwrap();
            let savings = amounts.get(1).unwrap();
            let bills = amounts.get(2).unwrap();
            let insurance = amounts.get(3).unwrap();

            let sum = spending + savings + bills + insurance;
            assert_eq!(
                sum, amount,
                "Sum mismatch for large amount: {} != {}",
                sum, amount
            );
        }
        // Else overflow is acceptable for very large amounts
    }
}

/// Test all valid single-category splits (100% to one category)
#[test]
fn fuzz_single_category_splits() {
    let single_category_splits = vec![
        (100, 0, 0, 0), // All to spending
        (0, 100, 0, 0), // All to savings
        (0, 0, 100, 0), // All to bills
        (0, 0, 0, 100), // All to insurance
    ];

    for (spending_pct, savings_pct, bills_pct, insurance_pct) in single_category_splits {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.initialize_split(
            &owner,
            &0,
            &spending_pct,
            &savings_pct,
            &bills_pct,
            &insurance_pct,
        );

        let amounts = client.calculate_split(&1000);

        let spending = amounts.get(0).unwrap();
        let savings = amounts.get(1).unwrap();
        let bills = amounts.get(2).unwrap();
        let insurance = amounts.get(3).unwrap();

        // One should be 1000, others should be 0
        assert_eq!(spending + savings + bills + insurance, 1000);

        if spending_pct == 100 {
            assert_eq!(spending, 1000);
            assert_eq!(savings + bills + insurance, 0);
        } else if savings_pct == 100 {
            assert_eq!(savings, 1000);
            assert_eq!(spending + bills + insurance, 0);
        } else if bills_pct == 100 {
            assert_eq!(bills, 1000);
            assert_eq!(spending + savings + insurance, 0);
        } else if insurance_pct == 100 {
            assert_eq!(insurance, 1000);
            assert_eq!(spending + savings + bills, 0);
        }
    }
}
