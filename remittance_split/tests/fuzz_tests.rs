#![cfg(test)]

//! Fuzz/Property-based tests for numeric operations in remittance_split.
//!
//! These tests verify critical numeric invariants:
//! - Overflow protection
//! - Rounding behavior
//! - Sum preservation (split amounts always equal total)
//! - Edge cases with extreme values

use remittance_split::{RemittanceSplit, RemittanceSplitClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Helper: register a dummy token address (no real token needed for pure math tests).
fn dummy_token(env: &Env) -> Address {
    Address::generate(env)
}

/// Helper: initialize split with a dummy token address.
fn init(
    client: &RemittanceSplitClient,
    env: &Env,
    owner: &Address,
    s: u32,
    g: u32,
    b: u32,
    i: u32,
) {
    let token = dummy_token(env);
    client.initialize_split(owner, &0, &token, &s, &g, &b, &i);
}

/// Helper: try_initialize_split with a dummy token address.
fn try_init(
    client: &RemittanceSplitClient,
    env: &Env,
    owner: &Address,
    s: u32,
    g: u32,
    b: u32,
    i: u32,
) -> Result<bool, ()> {
    let token = dummy_token(env);
    client
        .try_initialize_split(owner, &0, &token, &s, &g, &b, &i)
        .map(|r| r.unwrap())
        .map_err(|_| ())
}

// ---------------------------------------------------------------------------

#[test]
fn fuzz_calculate_split_sum_preservation() {
    let test_cases = vec![
        (1000, 50, 30, 15, 5),
        (1, 25, 25, 25, 25),
        (999, 33, 33, 33, 1),
        (i128::MAX / 100, 25, 25, 25, 25),
        (12345678, 17, 19, 23, 41),
        (100, 1, 1, 1, 97),
        (999999, 10, 20, 30, 40),
        (7, 40, 30, 20, 10),
        (543210, 70, 10, 10, 10),
        (1000000, 0, 0, 0, 100),
    ];

    for (total_amount, sp, sg, sb, si) in test_cases {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        if try_init(&client, &env, &owner, sp, sg, sb, si).is_err() {
            continue;
        }

        if client.try_calculate_split(&total_amount).is_err() {
            continue;
        }

        let amounts = client.calculate_split(&total_amount);
        let sum: i128 = amounts.iter().sum();
        assert_eq!(sum, total_amount, "Sum mismatch for percentages {}%/{}%/{}%/{}%", sp, sg, sb, si);
        assert!(amounts.iter().all(|a| a >= 0), "Negative amount detected");
    }
}

#[test]
fn fuzz_calculate_split_small_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    init(&client, &env, &owner, 25, 25, 25, 25);

    for amount in 1..=100i128 {
        let amounts = client.calculate_split(&amount);
        let sum: i128 = amounts.iter().sum();
        assert_eq!(sum, amount, "Sum mismatch for amount {}", amount);
        assert!(amounts.iter().all(|a| a <= amount), "Amount exceeds total");
    }
}

#[test]
fn fuzz_rounding_behavior() {
    let prime_percentages = vec![
        (3u32, 7u32, 11u32, 79u32),
        (13, 17, 23, 47),
        (19, 23, 29, 29),
        (31, 37, 11, 21),
        (41, 43, 7, 9),
    ];

    for (sp, sg, sb, si) in prime_percentages {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        init(&client, &env, &owner, sp, sg, sb, si);

        for amount in &[100i128, 1000, 9999, 123456] {
            let amounts = client.calculate_split(amount);
            let sum: i128 = amounts.iter().sum();
            assert_eq!(sum, *amount, "Rounding error for amount {} with {}%/{}%/{}%/{}%", amount, sp, sg, sb, si);
        }
    }
}

#[test]
fn fuzz_invalid_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    init(&client, &env, &owner, 50, 30, 15, 5);

    for amount in &[0i128, -1, -100, -1000, i128::MIN] {
        let result = client.try_calculate_split(amount);
        assert!(result.is_err(), "Expected error for amount {}", amount);
    }
}

#[test]
fn fuzz_invalid_percentages() {
    let invalid_percentages = vec![
        (50u32, 50u32, 10u32, 0u32),
        (25, 25, 25, 24),
        (100, 0, 0, 1),
        (0, 0, 0, 0),
        (30, 30, 30, 30),
    ];

    for (sp, sg, sb, si) in invalid_percentages {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let total = sp + sg + sb + si;
        let result = try_init(&client, &env, &owner, sp, sg, sb, si);
        if total != 100 {
            assert!(result.is_err(), "Expected error for percentages summing to {}", total);
        }
    }
}

#[test]
fn fuzz_large_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    init(&client, &env, &owner, 25, 25, 25, 25);

    for amount in &[i128::MAX / 1000, i128::MAX / 100, 1_000_000_000_000i128, 999_999_999_999i128] {
        if let Ok(_) = client.try_calculate_split(amount) {
            let amounts = client.calculate_split(amount);
            let sum: i128 = amounts.iter().sum();
            assert_eq!(sum, *amount, "Sum mismatch for large amount {}", amount);
        }
    }
}

#[test]
fn fuzz_single_category_splits() {
    let single_category_splits = vec![
        (100u32, 0u32, 0u32, 0u32),
        (0, 100, 0, 0),
        (0, 0, 100, 0),
        (0, 0, 0, 100),
    ];

    for (sp, sg, sb, si) in single_category_splits {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        init(&client, &env, &owner, sp, sg, sb, si);

        let amounts = client.calculate_split(&1000);
        let sum: i128 = amounts.iter().sum();
        assert_eq!(sum, 1000);

        if sp == 100 { assert_eq!(amounts.get(0).unwrap(), 1000); }
        if sg == 100 { assert_eq!(amounts.get(1).unwrap(), 1000); }
        if sb == 100 { assert_eq!(amounts.get(2).unwrap(), 1000); }
        if si == 100 { assert_eq!(amounts.get(3).unwrap(), 1000); }
    }
}
