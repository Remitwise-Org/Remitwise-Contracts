#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_initialize_split() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let success = client.initialize_split(
        &owner,
        &50, // spending
        &30, // savings
        &15, // bills
        &5,  // insurance
    );

    assert!(success);

    let config = client.get_config().unwrap();
    assert_eq!(config.owner, owner);
    assert_eq!(config.spending_percent, 50);
    assert_eq!(config.savings_percent, 30);
    assert_eq!(config.bills_percent, 15);
    assert_eq!(config.insurance_percent, 5);
}

#[test]
#[should_panic(expected = "Percentages must sum to 100")]
fn test_initialize_split_invalid_sum() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    client.initialize_split(
        &owner,
        &50, 
        &50, 
        &10, // Sums to 110
        &0,  
    );
}

#[test]
#[should_panic(expected = "Split already initialized. Use update_split to modify.")]
fn test_initialize_split_already_initialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &50, &30, &15, &5);
    // Second init should fail
    client.initialize_split(&owner, &50, &30, &15, &5);
}

#[test]
fn test_update_split() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &50, &30, &15, &5);

    let success = client.update_split(
        &owner,
        &40,
        &40,
        &10,
        &10
    );
    assert!(success);

    let config = client.get_config().unwrap();
    assert_eq!(config.spending_percent, 40);
    assert_eq!(config.savings_percent, 40);
    assert_eq!(config.bills_percent, 10);
    assert_eq!(config.insurance_percent, 10);
}

#[test]
#[should_panic(expected = "Only the owner can update the split configuration")]
fn test_update_split_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &50, &30, &15, &5);

    client.update_split(&other, &40, &40, &10, &10);
}

#[test]
fn test_calculate_split() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &50, &30, &15, &5);

    // Test with 1000 units
    let amounts = client.calculate_split(&1000);
    
    // spending: 50% of 1000 = 500
    // savings: 30% of 1000 = 300
    // bills: 15% of 1000 = 150
    // insurance: remainder = 1000 - 500 - 300 - 150 = 50
    
    assert_eq!(amounts.get(0).unwrap(), 500);
    assert_eq!(amounts.get(1).unwrap(), 300);
    assert_eq!(amounts.get(2).unwrap(), 150);
    assert_eq!(amounts.get(3).unwrap(), 50);
}

#[test]
fn test_calculate_split_rounding() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    // 33, 33, 33, 1 setup
    client.initialize_split(&owner, &33, &33, &33, &1);

    // Total 100
    // 33% = 33
    // Remainder should go to last one (insurance) logic in contract: 
    // insurance = total - spending - savings - bills
    // 100 - 33 - 33 - 33 = 1. Correct.
    
    let amounts = client.calculate_split(&100);
    assert_eq!(amounts.get(0).unwrap(), 33);
    assert_eq!(amounts.get(1).unwrap(), 33);
    assert_eq!(amounts.get(2).unwrap(), 33);
    assert_eq!(amounts.get(3).unwrap(), 1);
}

#[test]
#[should_panic(expected = "Total amount must be positive")]
fn test_calculate_split_zero_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();
    client.initialize_split(&owner, &50, &30, &15, &5);
    
    client.calculate_split(&0);
}

#[test]
fn test_calculate_complex_rounding() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();
    // 17, 19, 23, 41 (Primes summing to 100)
    client.initialize_split(&owner, &17, &19, &23, &41);
    
    // Amount 1000
    // 17% = 170
    // 19% = 190
    // 23% = 230
    // 41% = 410
    // Sum = 1000. Perfect.
    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 170);
    assert_eq!(amounts.get(1).unwrap(), 190);
    assert_eq!(amounts.get(2).unwrap(), 230);
    assert_eq!(amounts.get(3).unwrap(), 410);

    // Amount 3
    // 17% of 3 = 0
    // 19% of 3 = 0
    // 23% of 3 = 0
    // Remainder = 3 - 0 - 0 - 0 = 3. All goes to insurance.
    let tiny_amounts = client.calculate_split(&3);
    assert_eq!(tiny_amounts.get(0).unwrap(), 0);
    assert_eq!(tiny_amounts.get(3).unwrap(), 3);
}
