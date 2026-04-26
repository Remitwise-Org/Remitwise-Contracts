#![cfg(test)]

use emergency_killswitch::{EmergencyKillswitch, EmergencyKillswitchClient};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    Address, Env, Symbol,
};

#[test]
fn test_unauthorized_emergency_trigger() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let unauthorized = Address::generate(&env);

    // We expect a panic when require_auth fails if mock_all_auths is not set
    // or we can use mock_auths to simulate a different caller.
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

    // Advance ledger to unpause time
    env.ledger().set_timestamp(future);
    client.unpause();
    assert!(!client.is_paused());
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
#[should_panic(expected = "max paused functions reached")]
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

    client.pause_function(&module, &symbol_short!("one_more"));
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
