#![cfg(test)]

use emergency_killswitch::{EmergencyKillswitch, EmergencyKillswitchClient, Error};
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

    let _unauthorized = Address::generate(&env);

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
fn test_premature_unpause_rejection() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.pause();
    assert!(client.is_paused());

    // Schedule unpause in the future
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);

    // Try to unpause before scheduled time (1 second before)
    env.ledger().set_timestamp(future - 1);
    let result = client.try_unpause();
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
    assert!(client.is_paused());

    // Unpause at exact boundary
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
    assert!(client.is_paused());

    // Schedule unpause in the future
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);

    // Call pause again (this should cancel/reset the pending schedule)
    client.pause();

    // Advance ledger to the previously scheduled future time
    env.ledger().set_timestamp(future);

    // Try to unpause. It should fail because the schedule was cancelled/removed on re-pause.
    let result = client.try_unpause();
    assert_eq!(result, Err(Ok(Error::InvalidSchedule)));
    assert!(client.is_paused());
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
    assert!(client.is_paused());

    // Set initial ledger time
    env.ledger().set_timestamp(1000);

    // Try to schedule unpause with a past timestamp (e.g. 999)
    let result1 = client.try_schedule_unpause(&999);
    assert_eq!(result1, Err(Ok(Error::InvalidSchedule)));

    // Try to schedule unpause with the exact current timestamp (1000) - this is allowed/valid
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

/// Verify that a function pause works independently when its module is not paused.
#[test]
fn test_function_pause_independent_when_module_not_paused() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let module = symbol_short!("bill");
    let paused_fn = symbol_short!("pay");
    let other_fn = symbol_short!("refund");

    assert!(!client.is_function_paused(&module, &paused_fn));
    assert!(!client.is_function_paused(&module, &other_fn));

    client.pause_function(&module, &paused_fn);

    assert!(client.is_function_paused(&module, &paused_fn));
    assert!(!client.is_function_paused(&module, &other_fn));

    client.unpause_function(&module, &paused_fn);
    assert!(!client.is_function_paused(&module, &paused_fn));
}

/// Assert that module-level pause overrides function-level flags and that per-function state is preserved across module unpause.
#[test]
fn test_module_pause_precedence_over_function_and_restore_on_unpause() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let module = symbol_short!("bill");
    let paused_fn = symbol_short!("pay");
    let unpaused_fn = symbol_short!("refund");

    assert!(!client.is_function_paused(&module, &paused_fn));
    assert!(!client.is_function_paused(&module, &unpaused_fn));

    client.pause_function(&module, &paused_fn);
    assert!(client.is_function_paused(&module, &paused_fn));
    assert!(!client.is_function_paused(&module, &unpaused_fn));

    client.pause_module(&module);
    assert!(client.is_function_paused(&module, &paused_fn));
    assert!(client.is_function_paused(&module, &unpaused_fn));

    client.unpause_module(&module);
    assert!(client.is_function_paused(&module, &paused_fn));
    assert!(!client.is_function_paused(&module, &unpaused_fn));
}

/// Assert that a global pause dominates both module and function pause scopes.
#[test]
fn test_global_pause_dominates_module_and_function_pause() {
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
    client.pause_module(&module);
    assert!(client.is_function_paused(&module, &paused_fn));
    assert!(client.is_function_paused(&module, &other_fn));

    client.pause();
    assert!(client.is_paused());
    assert!(client.is_function_paused(&module, &paused_fn));
    assert!(client.is_function_paused(&module, &other_fn));
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

    let result = client.try_pause_function(&module, &symbol_short!("one_more"));
    assert!(result.is_err());
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
