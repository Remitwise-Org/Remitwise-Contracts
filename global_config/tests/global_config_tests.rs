use global_config::{ConfigError, ConfigValue, GlobalConfig, GlobalConfigClient};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig};
use soroban_sdk::{Address, Env, Symbol};

fn make_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    env
}

fn deploy(env: &Env) -> (Address, GlobalConfigClient<'_>) {
    let id = env.register_contract(None, GlobalConfig);
    let client = GlobalConfigClient::new(env, &id);
    (id, client)
}

// -----------------------------------------------------------------------
// Initialization
// -----------------------------------------------------------------------

#[test]
fn test_initialize_sets_admin() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    assert_eq!(client.get_admin(), Some(admin));
}

#[test]
fn test_initialize_sets_version() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    assert_eq!(client.version(), global_config::CONTRACT_VERSION);
}

#[test]
fn test_double_initialize_fails() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);
    let result = client.try_initialize(&admin);
    assert_eq!(result, Err(Ok(ConfigError::AlreadyInitialized)));
}

// -----------------------------------------------------------------------
// Admin management
// -----------------------------------------------------------------------

#[test]
fn test_set_admin_transfers_control() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);

    client.initialize(&admin);
    client.set_admin(&admin, &new_admin);

    assert_eq!(client.get_admin(), Some(new_admin.clone()));

    // Old admin can no longer write
    let result = client.try_set_config(
        &admin,
        &Symbol::new(&env, "some_key"),
        &ConfigValue::U32(1),
    );
    assert_eq!(result, Err(Ok(ConfigError::Unauthorized)));
}

#[test]
fn test_non_admin_set_admin_fails() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.initialize(&admin);

    let result = client.try_set_admin(&stranger, &stranger);
    assert_eq!(result, Err(Ok(ConfigError::Unauthorized)));
}

// -----------------------------------------------------------------------
// set_config / get_config
// -----------------------------------------------------------------------

#[test]
fn test_set_and_get_u32() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let key = Symbol::new(&env, global_config::config_keys::MAX_PAGE_LIMIT);
    client.set_config(&admin, &key, &ConfigValue::U32(30));

    match client.get_config(&key) {
        Some(ConfigValue::U32(v)) => assert_eq!(v, 30),
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn test_set_and_get_i128() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let key = Symbol::new(&env, "min_amount");
    client.set_config(&admin, &key, &ConfigValue::I128(1_000_000));

    match client.get_config(&key) {
        Some(ConfigValue::I128(v)) => assert_eq!(v, 1_000_000),
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn test_set_and_get_bool() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let key = Symbol::new(&env, "feature_flag");
    client.set_config(&admin, &key, &ConfigValue::Bool(true));

    match client.get_config(&key) {
        Some(ConfigValue::Bool(v)) => assert!(v),
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn test_get_nonexistent_key_returns_none() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let result = client.get_config(&Symbol::new(&env, "no_such_key"));
    assert!(result.is_none());
}

#[test]
fn test_overwrite_config_value() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);
    let key = Symbol::new(&env, "my_limit");

    client.set_config(&admin, &key, &ConfigValue::U32(10));
    client.set_config(&admin, &key, &ConfigValue::U32(20));

    match client.get_config(&key) {
        Some(ConfigValue::U32(v)) => assert_eq!(v, 20),
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn test_unauthorized_set_config_fails() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.initialize(&admin);

    let result = client.try_set_config(
        &stranger,
        &Symbol::new(&env, "key"),
        &ConfigValue::U32(1),
    );
    assert_eq!(result, Err(Ok(ConfigError::Unauthorized)));
}

// -----------------------------------------------------------------------
// get_all_keys
// -----------------------------------------------------------------------

#[test]
fn test_get_all_keys_empty_after_init() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    assert_eq!(client.get_all_keys().len(), 0);
}

#[test]
fn test_get_all_keys_lists_set_keys() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let k1 = Symbol::new(&env, global_config::config_keys::MAX_PAGE_LIMIT);
    let k2 = Symbol::new(&env, global_config::config_keys::MAX_BATCH_SIZE);
    let k3 = Symbol::new(&env, global_config::config_keys::MAX_AUDIT_ENTRIES);

    client.set_config(&admin, &k1, &ConfigValue::U32(50));
    client.set_config(&admin, &k2, &ConfigValue::U32(50));
    client.set_config(&admin, &k3, &ConfigValue::U32(100));

    assert_eq!(client.get_all_keys().len(), 3);
}

// -----------------------------------------------------------------------
// Pre-init guard
// -----------------------------------------------------------------------

#[test]
fn test_set_config_before_init_fails() {
    let env = make_env();
    let (_, client) = deploy(&env);
    let caller = Address::generate(&env);

    let result = client.try_set_config(
        &caller,
        &Symbol::new(&env, "key"),
        &ConfigValue::U32(1),
    );
    assert_eq!(result, Err(Ok(ConfigError::NotInitialized)));
}
