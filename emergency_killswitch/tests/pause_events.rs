#![cfg(test)]
use soroban_sdk::{testutils::Address as _, Env, Symbol};

use crate::{EmergencyKillswitch, EmergencyKillswitchClient};

#[test]
fn test_pause_audit_trail_events_in_expected_order() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);

    let admin = env.register_stellar_address();
    client.initialize(&admin);

    // Global pause
    client.pause();
    let events = env.events().all();
    assert_eq!(events.len(), 1); // pause event

    let (topics, data) = events.get(0).unwrap();
    assert_eq!(topics, (symbol_short!("emergency"), symbol_short!("paused")));
    // Verify data matches (GLOBAL, timestamp)

    // Module pause
    let module = symbol_short!("remit");
    client.pause_module(&module);
    // Assert event order + content

    // Function pause
    let func = symbol_short!("pay");
    client.pause_function(&module, &func);
    // ...

    // Unpause flow, clear_emergency_state, etc.
}