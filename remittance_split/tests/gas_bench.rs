use remittance_split::{AccountGroup, RemittanceSplit, RemittanceSplitClient};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Env};

fn bench_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 1,
        timestamp: 1_700_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100_000,
    });
    let mut budget = env.budget();
    budget.reset_unlimited();
    env
}

fn measure<F, R>(env: &Env, f: F) -> (u64, u64, R)
where
    F: FnOnce() -> R,
{
    let mut budget = env.budget();
    budget.reset_unlimited();
    budget.reset_tracker();
    let result = f();
    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();
    (cpu, mem, result)
}

#[test]
fn bench_distribute_usdc_worst_case() {
    let env = bench_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let admin = <Address as AddressTrait>::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());

    let payer = <Address as AddressTrait>::generate(&env);
    let amount = 10_000i128;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&payer, &amount);

    let accounts = AccountGroup {
        spending: <Address as AddressTrait>::generate(&env),
        savings: <Address as AddressTrait>::generate(&env),
        bills: <Address as AddressTrait>::generate(&env),
        insurance: <Address as AddressTrait>::generate(&env),
    };

    let (cpu, mem, distributed) = measure(&env, || {
        client.distribute_usdc(&token_contract.address(), &payer, &accounts, &amount)
    });
    assert!(distributed);

    println!(
        r#"{{"contract":"remittance_split","method":"distribute_usdc","scenario":"4_recipients_all_nonzero","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}
