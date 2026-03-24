#![no_std]
use soroban_sdk::{
    testutils::{Address as AddressTrait, Ledger, LedgerInfo},
    Address, Env,
};

pub fn set_ledger_time(env: &Env, timestamp: u64) {
    env.ledger().set(LedgerInfo {
        protocol_version: 22,
        sequence_number: 1,
        timestamp,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 3_110_400,
    });
}

pub fn generate_test_address(env: &Env) -> Address {
    Address::generate(env)
}

#[macro_export]
macro_rules! setup_test_env {
    ($env:ident, $contract:ident, $client_type:ident, $client:ident, $owner:ident) => {
        let $env = Env::default();
        $env.mock_all_auths();
        let contract_id = $env.register($contract, ());
        let $client = $client_type::new(&$env, &contract_id);
        let $owner = $crate::generate_test_address(&$env);
    };
}
