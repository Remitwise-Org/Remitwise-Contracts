#[cfg(not(target_arch = "wasm32"))]
pub mod tests {
    use soroban_sdk::Env;
    use testutils::set_ledger_time;
    use soroban_sdk::testutils::{Ledger, LedgerInfo};
    use soroban_sdk::Env;

    pub fn setup_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        set_ledger_time(&env, 1, 1704067200); // Jan 1, 2024
        env
    }
}
