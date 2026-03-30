#![no_std]
use soroban_sdk::{
    testutils::{Address as AddressTrait, Ledger, LedgerInfo},
    Address, Env,
};

pub fn set_ledger_time(env: &Env, sequence_number: u32, timestamp: u64) {
    let proto = env.ledger().protocol_version();

    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number,
        timestamp,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        // Must exceed any contract bump TTL used in tests (e.g. 518,400).
        max_entry_ttl: 3_000_000,
    });
}

pub fn generate_test_address(env: &Env) -> Address {
    Address::generate(env)
}

#[macro_export]
macro_rules! setup_test_env {
    ($env:ident, $contract:ident, $client_struct:ident, $client:ident, $owner:ident) => {
        let $env = Env::default();
        $env.mock_all_auths();
        let contract_id = $env.register_contract(None, $contract);
        let $client = SavingsGoalContractClient::new(&$env, &contract_id);
        let $client = $client_struct::new(&$env, &contract_id);
        let $owner = $crate::generate_test_address(&$env);
    };
}
