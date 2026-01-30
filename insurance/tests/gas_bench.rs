use insurance::{Insurance, InsuranceClient};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String};

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
fn bench_get_total_monthly_premium_worst_case() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    let name = String::from_str(&env, "BenchPolicy");
    let coverage_type = String::from_str(&env, "health");
    for _ in 0..100 {
        client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128);
    }

    let expected_total = 100i128 * 100i128;
    let (cpu, mem, total) = measure(&env, || client.get_total_monthly_premium(&owner));
    assert_eq!(total, expected_total);

    println!(
        r#"{{"contract":"insurance","method":"get_total_monthly_premium","scenario":"100_active_policies","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}
