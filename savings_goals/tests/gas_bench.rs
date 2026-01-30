use savings_goals::{SavingsGoalContract, SavingsGoalContractClient};
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
fn bench_get_all_goals_worst_case() {
    let env = bench_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    let name = String::from_str(&env, "BenchGoal");
    for _ in 0..100 {
        client.create_goal(&owner, &name, &1_000i128, &1_800_000u64);
    }

    let (cpu, mem, goals) = measure(&env, || client.get_all_goals(&owner));
    assert_eq!(goals.len(), 100);

    println!(
        r#"{{"contract":"savings_goals","method":"get_all_goals","scenario":"100_goals_single_owner","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}
