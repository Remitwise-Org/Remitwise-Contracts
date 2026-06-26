#![cfg(test)]

use emergency_killswitch::{EmergencyKillswitch, EmergencyKillswitchClient, Error};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    Address, Env, Symbol,
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
    assert_eq!(
        client.try_initialize(&contract_id),
        Err(Ok(Error::InvalidAdmin))
    );
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
    assert_eq!(
        client.try_transfer_admin(&contract_id),
        Err(Ok(Error::InvalidAdmin))
    );
}

#[test]
fn transfer_admin_rejects_same_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert_eq!(
        client.try_transfer_admin(&admin),
        Err(Ok(Error::InvalidAdmin))
    );
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
    assert_eq!(
        client.try_schedule_unpause(&999),
        Err(Ok(Error::InvalidSchedule))
    );
    client.schedule_unpause(&1000);
}

#[test]
fn test_clear_emergency_state_recovers_stuck_pause() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Reproduce the stuck-paused state: re-pause drops the schedule, so a
    // later unpause fails with InvalidSchedule even past the original time.
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    client.pause();
    env.ledger().set_timestamp(future);
    assert_eq!(client.try_unpause(), Err(Ok(Error::InvalidSchedule)));
    assert!(client.is_paused());

    // The recovery entrypoint lifts the pause immediately.
    client.clear_emergency_state();
    assert!(!client.is_paused());
}

#[test]
fn test_clear_emergency_state_bypasses_timelock() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);

    // Well before the scheduled time, unpause is rejected but clear is not.
    assert_eq!(client.try_unpause(), Err(Ok(Error::Unauthorized)));
    client.clear_emergency_state();
    assert!(!client.is_paused());

    // The pending schedule was wiped: a later unpause has nothing to act on.
    env.ledger().set_timestamp(future);
    assert_eq!(client.try_unpause(), Err(Ok(Error::InvalidSchedule)));
}

#[test]
fn test_clear_emergency_state_is_idempotent_when_active() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Safe no-op when the contract was never paused.
    assert!(!client.is_paused());
    client.clear_emergency_state();
    assert!(!client.is_paused());
}

#[test]
fn test_clear_emergency_state_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    assert_eq!(
        client.try_clear_emergency_state(),
        Err(Ok(Error::NotInitialized))
    );
}

#[test]
fn test_clear_emergency_state_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);
    client.pause();

    // Without mocked auth the admin requirement must reject the call.
    env.set_auths(&[]);
    assert!(client.try_clear_emergency_state().is_err());
    assert!(client.is_paused());
}

#[test]
fn test_clear_emergency_state_preserves_module_and_function_pauses() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");

    client.pause_module(&module);
    client.pause_function(&module, &func);
    client.pause();
    assert!(client.is_paused());

    client.clear_emergency_state();

    // Global pause is cleared, but the narrower scopes survive.
    assert!(!client.is_paused());
    assert!(client.is_function_paused(&module, &func));
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
    assert_eq!(
        client.try_pause_function(&module, &symbol_short!("one_more")),
        Err(Ok(Error::LimitExceeded))
    );
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

// ── get_unpause_schedule ────────────────────────────────────────────────────

#[test]
fn get_unpause_schedule_none_when_not_set() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert_eq!(client.get_unpause_schedule(), None);
}

#[test]
fn get_unpause_schedule_returns_scheduled_time() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    assert_eq!(client.get_unpause_schedule(), Some(future));
}

#[test]
fn get_unpause_schedule_none_after_pause_clears_it() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    // re-pause should clear the schedule
    client.pause();
    assert_eq!(client.get_unpause_schedule(), None);
}

#[test]
fn get_unpause_schedule_none_after_unpause_clears_it() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    env.ledger().set_timestamp(future);
    client.unpause();
    assert_eq!(client.get_unpause_schedule(), None);
}

// ── list_paused_functions ───────────────────────────────────────────────────

#[test]
fn list_paused_functions_empty_when_none_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    assert!(client.list_paused_functions(&module).is_empty());
}

#[test]
fn list_paused_functions_reflects_pause_then_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");
    client.pause_function(&module, &func);
    let list = client.list_paused_functions(&module);
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), func);
    client.unpause_function(&module, &func);
    assert!(client.list_paused_functions(&module).is_empty());
}

#[test]
fn list_paused_functions_multiple_functions() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let f1 = symbol_short!("pay");
    let f2 = symbol_short!("refund");
    client.pause_function(&module, &f1);
    client.pause_function(&module, &f2);
    let list = client.list_paused_functions(&module);
    assert_eq!(list.len(), 2);
    assert!(list.contains(f1));
    assert!(list.contains(f2));
}

#[test]
fn list_paused_functions_isolated_per_module() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let m1 = symbol_short!("bill");
    let m2 = symbol_short!("savings");
    let func = symbol_short!("pay");
    client.pause_function(&m1, &func);
    assert_eq!(client.list_paused_functions(&m1).len(), 1);
    assert!(client.list_paused_functions(&m2).is_empty());
}

// ── is_module_paused ────────────────────────────────────────────────────────

#[test]
fn is_module_paused_false_when_not_set() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert!(!client.is_module_paused(&symbol_short!("bill")));
}

#[test]
fn is_module_paused_true_after_pause_module() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    client.pause_module(&module);
    assert!(client.is_module_paused(&module));
}

#[test]
fn is_module_paused_false_after_unpause_module() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    client.pause_module(&module);
    client.unpause_module(&module);
    assert!(!client.is_module_paused(&module));
}

#[test]
fn is_module_paused_independent_of_global_pause() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    // Global pause does not set module-level flag
    client.pause();
    assert!(!client.is_module_paused(&module));
    // Module can be independently paused alongside global pause
    client.pause_module(&module);
    assert!(client.is_module_paused(&module));
}

#[test]
fn is_module_paused_does_not_affect_function_list() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    client.pause_module(&module);
    // Module being paused doesn't populate PausedFunctions
    assert!(client.list_paused_functions(&module).is_empty());
}
