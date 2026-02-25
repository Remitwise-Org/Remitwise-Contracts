pub mod tests {
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
    use soroban_sdk::{Address, Env};

    pub fn setup_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set(LedgerInfo {
            timestamp: 1704067200, // Jan 1, 2024
            protocol_version: 20,
            sequence_number: 1,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });
        env
    }
}
