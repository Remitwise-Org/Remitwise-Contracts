use orchestrator::{Orchestrator, OrchestratorClient};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env};

#[derive(Clone, Copy)]
struct RegressionSpec {
    cpu_baseline: u64,
    mem_baseline: u64,
    cpu_threshold_percent: u64,
    mem_threshold_percent: u64,
}

const INIT: RegressionSpec = RegressionSpec {
    cpu_baseline: 65241,
    mem_baseline: 9419,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const GET_NONCE: RegressionSpec = RegressionSpec {
    cpu_baseline: 43971,
    mem_baseline: 8270,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const GET_EXECUTION_STATS: RegressionSpec = RegressionSpec {
    cpu_baseline: 67048,
    mem_baseline: 11962,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const GET_PENDING_REWARDS: RegressionSpec = RegressionSpec {
    cpu_baseline: 45061,
    mem_baseline: 8369,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const GET_AUDIT_LOG: RegressionSpec = RegressionSpec {
    cpu_baseline: 46184,
    mem_baseline: 8507,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

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

fn make_addrs(env: &Env) -> [Address; 5] {
    [
        Address::generate(env),
        Address::generate(env),
        Address::generate(env),
        Address::generate(env),
        Address::generate(env),
    ]
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

fn assert_regression_bounds(
    method: &str,
    scenario: &str,
    cpu: u64,
    mem: u64,
    spec: RegressionSpec,
) {
    if spec.cpu_baseline > 0 {
        let cpu_max =
            spec.cpu_baseline + spec.cpu_baseline.saturating_mul(spec.cpu_threshold_percent) / 100;
        assert!(
            cpu <= cpu_max,
            "cpu regression for {}/{}: observed={}, allowed={} (baseline={}, threshold={}%)",
            method,
            scenario,
            cpu,
            cpu_max,
            spec.cpu_baseline,
            spec.cpu_threshold_percent
        );
    }
    if spec.mem_baseline > 0 {
        let mem_max =
            spec.mem_baseline + spec.mem_baseline.saturating_mul(spec.mem_threshold_percent) / 100;
        assert!(
            mem <= mem_max,
            "mem regression for {}/{}: observed={}, allowed={} (baseline={}, threshold={}%)",
            method,
            scenario,
            mem,
            mem_max,
            spec.mem_baseline,
            spec.mem_threshold_percent
        );
    }
}

fn emit_bench_result(method: &str, scenario: &str, cpu: u64, mem: u64, spec: RegressionSpec) {
    println!(
        "GAS_BENCH_RESULT {{\"contract\":\"orchestrator\",\"method\":\"{}\",\"scenario\":\"{}\",\"cpu\":{},\"mem\":{},\"cpu_baseline\":{},\"mem_baseline\":{},\"cpu_threshold_percent\":{},\"mem_threshold_percent\":{}}}",
        method,
        scenario,
        cpu,
        mem,
        spec.cpu_baseline,
        spec.mem_baseline,
        spec.cpu_threshold_percent,
        spec.mem_threshold_percent
    );
}

#[test]
fn bench_orchestrator_init() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &contract_id);
    let caller = Address::generate(&env);
    let addrs = make_addrs(&env);
    let (cpu, mem, _) = measure(&env, || {
        client.init(&caller, &addrs[0], &addrs[1], &addrs[2], &addrs[3], &addrs[4])
    });

    emit_bench_result("init", "initialized", cpu, mem, INIT);
    assert_regression_bounds("init", "initialized", cpu, mem, INIT);
}

#[test]
fn bench_orchestrator_get_nonce() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &contract_id);
    let caller = Address::generate(&env);
    let addrs = make_addrs(&env);
    client.init(&caller, &addrs[0], &addrs[1], &addrs[2], &addrs[3], &addrs[4]);

    let (cpu, mem, nonce) = measure(&env, || client.get_nonce(&caller));
    assert_eq!(nonce, 0);

    emit_bench_result("get_nonce", "fresh_address", cpu, mem, GET_NONCE);
    assert_regression_bounds("get_nonce", "fresh_address", cpu, mem, GET_NONCE);
}

#[test]
fn bench_orchestrator_get_execution_stats() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &contract_id);
    let caller = Address::generate(&env);
    let addrs = make_addrs(&env);
    client.init(&caller, &addrs[0], &addrs[1], &addrs[2], &addrs[3], &addrs[4]);

    let (cpu, mem, stats) = measure(&env, || client.get_execution_stats());
    assert!(stats.is_some());
    assert_eq!(stats.unwrap().total_executions, 0);

    emit_bench_result(
        "get_execution_stats",
        "fresh_contract",
        cpu,
        mem,
        GET_EXECUTION_STATS,
    );
    assert_regression_bounds(
        "get_execution_stats",
        "fresh_contract",
        cpu,
        mem,
        GET_EXECUTION_STATS,
    );
}

#[test]
fn bench_orchestrator_get_pending_rewards() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &contract_id);
    let caller = Address::generate(&env);
    let addrs = make_addrs(&env);
    client.init(&caller, &addrs[0], &addrs[1], &addrs[2], &addrs[3], &addrs[4]);

    let (cpu, mem, rewards) = measure(&env, || client.get_pending_rewards(&caller));
    assert_eq!(rewards, 0);

    emit_bench_result(
        "get_pending_rewards",
        "zero_balance",
        cpu,
        mem,
        GET_PENDING_REWARDS,
    );
    assert_regression_bounds(
        "get_pending_rewards",
        "zero_balance",
        cpu,
        mem,
        GET_PENDING_REWARDS,
    );
}

#[test]
fn bench_orchestrator_get_audit_log() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &contract_id);
    let caller = Address::generate(&env);
    let addrs = make_addrs(&env);
    client.init(&caller, &addrs[0], &addrs[1], &addrs[2], &addrs[3], &addrs[4]);

    let (cpu, mem, log) = measure(&env, || client.get_audit_log(&0u32, &20u32));
    assert_eq!(log.len(), 0);

    emit_bench_result("get_audit_log", "empty_log", cpu, mem, GET_AUDIT_LOG);
    assert_regression_bounds("get_audit_log", "empty_log", cpu, mem, GET_AUDIT_LOG);
}
