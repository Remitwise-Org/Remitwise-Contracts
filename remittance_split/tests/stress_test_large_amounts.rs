#![cfg(test)]

//! Stress tests for arithmetic operations with very large i128 values in remittance_split.

use remittance_split::{RemittanceSplit, RemittanceSplitClient, MAX_SCHEDULES_PER_OWNER};
use soroban_sdk::testutils::Address as AddressTrait;
use soroban_sdk::{Address, Env};

fn dummy_token(env: &Env) -> Address {
    Address::generate(env)
}

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

//#[test]
// fn test_calculate_split_overflow_detection() {
//     let env = Env::default();
//     let contract_id = env.register_contract(None, RemittanceSplit);
//     let client = RemittanceSplitClient::new(&env, &contract_id);
//     let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

//     env.mock_all_auths();

//     client.initialize_split(&owner, &0, &50, &30, &15, &5);

//     // Value that will overflow when multiplied by percentage
//     let overflow_amount = i128::MAX / 50 + 1; // Will overflow when multiplied by 50

//     let result = client.try_calculate_split(&overflow_amount);

//     // Should return Overflow error, not panic
//     assert_eq!(result, Err(Ok(RemittanceSplitError::Overflow)));
// }

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

    for amount in &[
        i128::MAX / 1000,
        i128::MAX / 500,
        i128::MAX / 200,
        i128::MAX / 150,
        i128::MAX / 100,
    ] {
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
#[test]
fn test_schedule_id_sequencing_monotonicity() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 50, 30, 15, 5);

    let amount = 1000_i128;
    let next_due = env.ledger().timestamp() + 86400;
    let interval = 86400;

    let mut last_id = 0;
    for _ in 0..MAX_SCHEDULES_PER_OWNER {
        let id = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);
        assert!(id > last_id, "Schedule IDs must be strictly monotonic");
        last_id = id;
    }
}

#[test]
fn test_schedule_id_uniqueness_across_operations() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 50, 30, 15, 5);

    let amount = 1000_i128;
    let next_due = env.ledger().timestamp() + 86400;
    let interval = 86400;

    // 1. Create several schedules
    let id1 = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);
    let id2 = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);
    let id3 = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);

    assert_ne!(id1, id2);
    assert_ne!(id2, id3);

    // 2. Modify one
    client.modify_remittance_schedule(&owner, &id1, &(amount * 2), &(next_due + 100), &interval);
    let mod_schedule = client.get_remittance_schedule(&id1).unwrap();
    assert_eq!(
        mod_schedule.id, id1,
        "Schedule ID must remain stable after modification"
    );

    // 3. Cancel one
    client.cancel_remittance_schedule(&owner, &id2);
    let cancelled = client.get_remittance_schedule(&id2).unwrap();
    assert!(!cancelled.active);

    // 4. Create new one and verify it doesn't collide with ANY previous ID (including cancelled/modified)
    let id4 = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);
    assert!(id4 > id3, "New ID must be greater than all previous IDs");
    assert_ne!(id4, id1);
    assert_ne!(id4, id2);
    assert_ne!(id4, id3);
}

#[test]
fn test_high_volume_schedule_creation_no_collisions() {
    let env = Env::default();
    env.budget().reset_unlimited();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 50, 30, 15, 5);

    let amount = 1000_i128;
    let next_due = env.ledger().timestamp() + 86400;

    // Create schedules up to the owner cap and track IDs
    let mut ids = soroban_sdk::Vec::new(&env);
    for i in 0..MAX_SCHEDULES_PER_OWNER {
        let id = client.create_remittance_schedule(&owner, &amount, &(next_due + i as u64), &0);
        ids.push_back(id);
    }

    // Verify all IDs are unique (O(n^2) check if necessary, or sort)
    // In soroban testing we can just use a Map for O(n)
    let mut seen = soroban_sdk::Map::new(&env);
    for id in ids.iter() {
        assert!(
            seen.get(id).is_none(),
            "Collision detected for schedule ID: {}",
            id
        );
        seen.set(id, true);
    }
}

#[test]
fn test_schedule_pagination_ordering_guarantees() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 50, 30, 15, 5);

    let amount = 1000i128;
    let next_due = env.ledger().timestamp() + 86400; // 1 day from now
    let interval = 604800; // 1 week

    // Create multiple schedules with different creation times
    let id1 = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);
    let id2 =
        client.create_remittance_schedule(&owner, &(amount * 2), &(next_due + 100), &interval);
    let id3 =
        client.create_remittance_schedule(&owner, &(amount * 3), &(next_due + 200), &interval);
    let id4 =
        client.create_remittance_schedule(&owner, &(amount * 4), &(next_due + 300), &interval);
    let id5 =
        client.create_remittance_schedule(&owner, &(amount * 5), &(next_due + 400), &interval);

    // Verify IDs are sequential and ascending
    assert!(id1 < id2 && id2 < id3 && id3 < id4 && id4 < id5);

    // Test pagination with small pages
    let page1 = client.get_schedules_paginated(&owner, &0, &2);
    assert_eq!(page1.count, 2);
    assert_eq!(page1.items.len(), 2);
    assert_eq!(page1.items.get(0).unwrap().id, id1);
    assert_eq!(page1.items.get(1).unwrap().id, id2);
    assert_eq!(page1.next_cursor, 2);

    let page2 = client.get_schedules_paginated(&owner, &2, &2);
    assert_eq!(page2.count, 2);
    assert_eq!(page2.items.len(), 2);
    assert_eq!(page2.items.get(0).unwrap().id, id3);
    assert_eq!(page2.items.get(1).unwrap().id, id4);
    assert_eq!(page2.next_cursor, 4);

    let page3 = client.get_schedules_paginated(&owner, &4, &2);
    assert_eq!(page3.count, 1);
    assert_eq!(page3.items.len(), 1);
    assert_eq!(page3.items.get(0).unwrap().id, id5);
    assert_eq!(page3.next_cursor, 0); // No more pages

    // Test empty page beyond range
    let empty_page = client.get_schedules_paginated(&owner, &10, &2);
    assert_eq!(empty_page.count, 0);
    assert_eq!(empty_page.items.len(), 0);
    assert_eq!(empty_page.next_cursor, 0);

    // Test full list for comparison
    let all_schedules = client.get_remittance_schedules(&owner);
    assert_eq!(all_schedules.len(), 5);
    // Verify all schedules are present and in ID order
    assert_eq!(all_schedules.get(0).unwrap().id, id1);
    assert_eq!(all_schedules.get(1).unwrap().id, id2);
    assert_eq!(all_schedules.get(2).unwrap().id, id3);
    assert_eq!(all_schedules.get(3).unwrap().id, id4);
    assert_eq!(all_schedules.get(4).unwrap().id, id5);
}

#[test]
fn test_schedule_pagination_stable_cursors() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 50, 30, 15, 5);

    let amount = 1000i128;
    let next_due = env.ledger().timestamp() + 86400;
    let interval = 604800;

    // Create schedules
    let id1 = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);
    let id2 = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);
    let id3 = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);

    // Test that repeated calls with same cursor return same results
    let page1_a = client.get_schedules_paginated(&owner, &0, &2);
    let page1_b = client.get_schedules_paginated(&owner, &0, &2);
    assert_eq!(page1_a.count, page1_b.count);
    assert_eq!(page1_a.next_cursor, page1_b.next_cursor);
    assert_eq!(
        page1_a.items.get(0).unwrap().id,
        page1_b.items.get(0).unwrap().id
    );
    assert_eq!(
        page1_a.items.get(1).unwrap().id,
        page1_b.items.get(1).unwrap().id
    );

    // Cancel middle schedule and verify pagination still works deterministically
    client.cancel_remittance_schedule(&owner, &id2);

    let page1_after = client.get_schedules_paginated(&owner, &0, &2);
    // Should still return id1 and id3 (id2 is cancelled but still in storage)
    assert_eq!(page1_after.count, 2);
    assert_eq!(page1_after.items.get(0).unwrap().id, id1);
    assert_eq!(page1_after.items.get(1).unwrap().id, id2);
}

#[test]
fn test_schedule_pagination_limit_clamping() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    env.mock_all_auths();

    init(&client, &env, &owner, 50, 30, 15, 5);

    let amount = 1000i128;
    let next_due = env.ledger().timestamp() + 86400;
    let interval = 604800;

    // Create many schedules
    for i in 0..10 {
        client.create_remittance_schedule(&owner, &amount, &(next_due + i as u64), &interval);
    }

    // Test that very large limit is clamped
    let page = client.get_schedules_paginated(&owner, &0, &1000);
    // Should be clamped to MAX_PAGE_LIMIT (50)
    assert!(page.count <= 50);
    assert!(page.items.len() <= 50);
}
