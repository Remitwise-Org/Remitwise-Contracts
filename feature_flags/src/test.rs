use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Env, String,
};

fn create_test_contract() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, FeatureFlagsContract);
    let admin = Address::generate(&env);

    (env, contract_id, admin)
}

#[test]
fn test_initialize() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    assert_eq!(client.get_admin(), admin);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_initialize_twice_fails() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.initialize(&admin);
}

#[test]
fn test_set_and_get_flag() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let key = String::from_str(&env, "strict_goal_dates");
    let description = String::from_str(&env, "Enforce strict date validation for savings goals");

    client.set_flag(&key, &true, &description);

    assert_eq!(client.is_enabled(&key), true);

    let flag = client.get_flag(&key).unwrap();
    assert_eq!(flag.key, key);
    assert_eq!(flag.enabled, true);
    assert_eq!(flag.description, description);
    assert_eq!(flag.updated_by, admin);
}

#[test]
fn test_is_enabled_returns_false_for_nonexistent_flag() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let key = String::from_str(&env, "nonexistent_flag");
    assert_eq!(client.is_enabled(&key), false);
}

#[test]
fn test_update_flag() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let key = String::from_str(&env, "test_feature");
    let desc1 = String::from_str(&env, "Initial description");
    let desc2 = String::from_str(&env, "Updated description");

    // Set flag to enabled
    client.set_flag(&key, &true, &desc1);
    assert_eq!(client.is_enabled(&key), true);

    // Update flag to disabled
    client.set_flag(&key, &false, &desc2);
    assert_eq!(client.is_enabled(&key), false);

    let flag = client.get_flag(&key).unwrap();
    assert_eq!(flag.description, desc2);
}

#[test]
fn test_delete_flag() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let key = String::from_str(&env, "temp_feature");
    let description = String::from_str(&env, "Temporary feature");

    client.set_flag(&key, &true, &description);
    assert_eq!(client.is_enabled(&key), true);

    client.delete_flag(&key);
    assert_eq!(client.is_enabled(&key), false);
    assert_eq!(client.get_flag(&key), None);
}

#[test]
#[should_panic(expected = "Feature flag not found")]
fn test_delete_nonexistent_flag() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let key = String::from_str(&env, "nonexistent");
    client.delete_flag(&key);
}

#[test]
fn test_get_all_flags() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let key1 = String::from_str(&env, "feature_1");
    let key2 = String::from_str(&env, "feature_2");
    let desc = String::from_str(&env, "Test feature");

    client.set_flag(&key1, &true, &desc);
    client.set_flag(&key2, &false, &desc);

    let all_flags = client.get_all_flags();
    assert_eq!(all_flags.len(), 2);
    assert!(all_flags.contains_key(key1.clone()));
    assert!(all_flags.contains_key(key2.clone()));
}

#[test]
fn test_update_admin() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let new_admin = Address::generate(&env);
    client.update_admin(&new_admin);

    assert_eq!(client.get_admin(), new_admin);
}

#[test]
fn test_multiple_flags_independent() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let key1 = String::from_str(&env, "strict_goal_dates");
    let key2 = String::from_str(&env, "auto_archive");
    let key3 = String::from_str(&env, "enhanced_validation");
    let desc = String::from_str(&env, "Test feature");

    client.set_flag(&key1, &true, &desc);
    client.set_flag(&key2, &false, &desc);
    client.set_flag(&key3, &true, &desc);

    assert_eq!(client.is_enabled(&key1), true);
    assert_eq!(client.is_enabled(&key2), false);
    assert_eq!(client.is_enabled(&key3), true);
}

#[test]
fn test_flag_events_emitted() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let key = String::from_str(&env, "test_feature");
    let description = String::from_str(&env, "Test feature");

    client.set_flag(&key, &true, &description);

    // Verify event was emitted
    let events = env.events().all();
    assert!(events.len() > 0);
}
