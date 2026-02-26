use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn create_test_contract() -> (Env, Address, Address) {
    let env = Env::default();
    let contract_id = env.register_contract(None, FeatureFlagsContract);
    let admin = Address::generate(&env);

    (env, contract_id, admin)
}

#[test]
fn test_initialize() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    client.initialize(&admin);

    assert!(client.is_initialized());
    assert_eq!(client.get_admin(), admin);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_initialize_twice_fails() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    client.initialize(&admin);
    client.initialize(&admin);
}

#[test]
fn test_set_flag() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    client.initialize(&admin);

    let key = String::from_str(&env, "strict_goal_dates");
    let description = String::from_str(&env, "Enforce future dates for savings goals");

    client.set_flag(&admin, &key, &true, &description);

    assert!(client.is_enabled(&key));

    let flag = client.get_flag(&key).unwrap();
    assert_eq!(flag.key, key);
    assert_eq!(flag.enabled, true);
    assert_eq!(flag.description, description);
    assert_eq!(flag.updated_by, admin);
}

#[test]
fn test_toggle_flag() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    client.initialize(&admin);

    let key = String::from_str(&env, "test_feature");
    let description = String::from_str(&env, "Test feature");

    // Enable
    client.set_flag(&admin, &key, &true, &description);
    assert!(client.is_enabled(&key));

    // Disable
    client.set_flag(&admin, &key, &false, &description);
    assert!(!client.is_enabled(&key));
}

#[test]
fn test_is_enabled_nonexistent_flag() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    client.initialize(&admin);

    let key = String::from_str(&env, "nonexistent");
    assert!(!client.is_enabled(&key));
}

#[test]
fn test_get_flag_nonexistent() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    client.initialize(&admin);

    let key = String::from_str(&env, "nonexistent");
    assert!(client.get_flag(&key).is_none());
}

#[test]
fn test_remove_flag() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    client.initialize(&admin);

    let key = String::from_str(&env, "test_feature");
    let description = String::from_str(&env, "Test feature");

    client.set_flag(&admin, &key, &true, &description);
    assert!(client.is_enabled(&key));

    client.remove_flag(&admin, &key);
    assert!(!client.is_enabled(&key));
    assert!(client.get_flag(&key).is_none());
}

#[test]
#[should_panic(expected = "Flag not found")]
fn test_remove_nonexistent_flag() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    client.initialize(&admin);

    let key = String::from_str(&env, "nonexistent");
    client.remove_flag(&admin, &key);
}

#[test]
fn test_get_all_flags() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    client.initialize(&admin);

    let key1 = String::from_str(&env, "feature1");
    let key2 = String::from_str(&env, "feature2");
    let desc = String::from_str(&env, "Description");

    client.set_flag(&admin, &key1, &true, &desc);
    client.set_flag(&admin, &key2, &false, &desc);

    let all_flags = client.get_all_flags();
    assert_eq!(all_flags.len(), 2);
    assert!(all_flags.contains_key(key1.clone()));
    assert!(all_flags.contains_key(key2.clone()));
}

#[test]
fn test_transfer_admin() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);
    let new_admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    client.transfer_admin(&admin, &new_admin);
    assert_eq!(client.get_admin(), new_admin);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_non_admin_cannot_set_flag() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);
    let non_admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let key = String::from_str(&env, "test");
    let desc = String::from_str(&env, "Test");

    client.set_flag(&non_admin, &key, &true, &desc);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_non_admin_cannot_remove_flag() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);
    let non_admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    let key = String::from_str(&env, "test");
    let desc = String::from_str(&env, "Test");
    client.set_flag(&admin, &key, &true, &desc);

    client.remove_flag(&non_admin, &key);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_non_admin_cannot_transfer_admin() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);
    let non_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    client.transfer_admin(&non_admin, &new_admin);
}

#[test]
#[should_panic(expected = "Flag key must be between 1 and 32 characters")]
fn test_empty_key_fails() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    client.initialize(&admin);

    let key = String::from_str(&env, "");
    let desc = String::from_str(&env, "Test");
    client.set_flag(&admin, &key, &true, &desc);
}

#[test]
#[should_panic(expected = "Flag key must be between 1 and 32 characters")]
fn test_long_key_fails() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    client.initialize(&admin);

    let key = String::from_str(&env, "this_is_a_very_long_key_that_exceeds_32_characters");
    let desc = String::from_str(&env, "Test");
    client.set_flag(&admin, &key, &true, &desc);
}

#[test]
#[should_panic(expected = "Description must be 256 characters or less")]
fn test_long_description_fails() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    client.initialize(&admin);

    let key = String::from_str(&env, "test");
    let desc = String::from_str(&env, &"a".repeat(257));
    client.set_flag(&admin, &key, &true, &desc);
}

#[test]
fn test_multiple_flags_independent() {
    let (env, contract_id, admin) = create_test_contract();
    let client = FeatureFlagsContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    client.initialize(&admin);

    let key1 = String::from_str(&env, "feature1");
    let key2 = String::from_str(&env, "feature2");
    let key3 = String::from_str(&env, "feature3");
    let desc = String::from_str(&env, "Test");

    client.set_flag(&admin, &key1, &true, &desc);
    client.set_flag(&admin, &key2, &false, &desc);
    client.set_flag(&admin, &key3, &true, &desc);

    assert!(client.is_enabled(&key1));
    assert!(!client.is_enabled(&key2));
    assert!(client.is_enabled(&key3));
}
