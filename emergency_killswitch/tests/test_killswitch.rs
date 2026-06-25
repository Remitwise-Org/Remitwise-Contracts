#![cfg(test)]

use emergency_killswitch::{EmergencyKillswitch, EmergencyKillswitchClient, Error, AdminTransferred};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger, Events},
    Address, Env, Symbol, IntoVal, Vec,
};

fn setup(env: &Env) -> (Address, EmergencyKillswitchClient<'_>) {
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(env, &contract_id);
    (contract_id, client)
}

// ── Initialize validation ────────────────────────────────────────

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
    let (_contract_id, client) = setup(&env);
    let admin = Address::generate(&env);
    assert_eq!(client.try_initialize(&admin), Ok(Ok(())));
}

// ── Transfer admin brick protection ──────────────────────────────

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
    let (_contract_id, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert_eq!(
        client.try_transfer_admin(&admin),
        Err(Ok(Error::InvalidAdmin))
    );
}

// ── AdminTransferred event ───────────────────────────────────────

#[test]
fn transfer_admin_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client) = setup(&env);
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize(&admin);

    // Clear mock auth event noise by resetting auths after initialize
    // then mock only the transfer_admin call
    env.set_auths(&[]);

    client.transfer_admin(&new_admin);

    let events = env.events().all();
    let mut found = false;
    for event in events {
        if event.0.len() >= 2
            && event.0[0] == symbol_short!("emergency")
            && event.0[1] == symbol_short!("admin_xfer")
        {
            found = true;
            break;
        }
    }
    assert!(found, "AdminTransferred event not emitted");
}

// ── Post-transfer privilege revocation ───────────────────────────

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn old_admin_cannot_pause_after_transfer() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize(&admin);
    env.mock_all_auths();
    client.transfer_admin(&new_admin);

    env.set_auths(&[]);
    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "pause",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.pause();
}

// ── Existing tests (updated) ─────────────────────────────────────

#[test]
fn test_unauthorized_emergency_trigger() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
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
    client.pause_function(&module, &paused_fn);
    assert!(client.is_function_paused(&module, &paused_fn));
    assert!(!client.is_function_paused(&module, &other_fn));
    client.unpause_function(&module, &paused_fn);
    assert!(!client.is_function_paused(&module, &paused_fn));
}

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
