#![cfg(test)]

use emergency_killswitch::{EmergencyKillswitch, EmergencyKillswitchClient, Error};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    Address, Env, Symbol, IntoVal,
};

fn setup(env: &Env) -> (Address, EmergencyKillswitchClient<'_>) {
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(env, &contract_id);
    (contract_id, client)
}

#[test]
fn initialize_rejects_self_address() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    assert_eq!(client.try_initialize(&contract_id), Err(Ok(Error::InvalidAdmin)));
}

#[test]
fn initialize_succeeds_with_valid_address() {
    let env = Env::default();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    assert_eq!(client.try_initialize(&admin), Ok(Ok(())));
}

#[test]
fn transfer_admin_rejects_self_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert_eq!(client.try_transfer_admin(&contract_id), Err(Ok(Error::InvalidAdmin)));
}

#[test]
fn transfer_admin_rejects_same_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert_eq!(client.try_transfer_admin(&admin), Err(Ok(Error::InvalidAdmin)));
}

#[test]
fn transfer_admin_succeeds_with_different_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize(&admin);
    assert_eq!(client.try_transfer_admin(&new_admin), Ok(Ok(())));
}

#[test]
fn test_authorized_emergency_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    assert!(client.is_paused());
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    env.ledger().set_timestamp(future);
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_premature_unpause_rejection() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    env.ledger().set_timestamp(future - 1);
    assert_eq!(client.try_unpause(), Err(Ok(Error::Unauthorized)));
    env.ledger().set_timestamp(future);
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_re_pause_cancels_schedule() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    client.pause();
    env.ledger().set_timestamp(future);
    assert_eq!(client.try_unpause(), Err(Ok(Error::InvalidSchedule)));
}

#[test]
fn test_timelock_bypass_rejection() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    env.ledger().set_timestamp(1000);
    assert_eq!(client.try_schedule_unpause(&999), Err(Ok(Error::InvalidSchedule)));
    client.schedule_unpause(&1000);
}

#[test]
fn test_per_function_pause() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");
    assert!(!client.is_function_paused(&module, &func));
    client.pause_function(&module, &func);
    assert!(client.is_function_paused(&module, &func));
    client.unpause_function(&module, &func);
    assert!(!client.is_function_paused(&module, &func));
}

#[test]
fn test_module_pause_precedence() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let paused_fn = symbol_short!("pay");
    let other_fn = symbol_short!("refund");
    client.pause_function(&module, &paused_fn);
    assert!(client.is_function_paused(&module, &paused_fn));
    assert!(!client.is_function_paused(&module, &other_fn));
    client.pause_module(&module);
    assert!(client.is_function_paused(&module, &other_fn));
    client.unpause_module(&module);
    assert!(client.is_function_paused(&module, &paused_fn));
    assert!(!client.is_function_paused(&module, &other_fn));
}

#[test]
fn test_global_pause_dominates() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");
    client.pause_function(&module, &func);
    client.pause_module(&module);
    client.pause();
    assert!(client.is_paused());
    assert!(client.is_function_paused(&module, &func));
}

#[test]
fn test_max_paused_functions_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    for i in 0..10 {
        client.pause_function(&module, &Symbol::new(&env, &format!("f{}", i)));
    }
    assert_eq!(client.try_pause_function(&module, &symbol_short!("one_more")), Err(Ok(Error::LimitExceeded)));
}

#[test]
fn test_module_pause() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");
    assert!(!client.is_function_paused(&module, &func));
    client.pause_module(&module);
    assert!(client.is_function_paused(&module, &func));
    client.unpause_module(&module);
    assert!(!client.is_function_paused(&module, &func));
}
