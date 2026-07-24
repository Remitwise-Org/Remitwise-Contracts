use emergency_killswitch::{EmergencyKillswitch, EmergencyKillswitchClient};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{symbol_short, Address, Env};

#[derive(Clone, Copy)]
struct RegressionSpec {
    cpu_baseline: u64,
    mem_baseline: u64,
    cpu_threshold_percent: u64,
    mem_threshold_percent: u64,
}

const INITIALIZE: RegressionSpec = RegressionSpec {
    cpu_baseline: 20769,
    mem_baseline: 2378,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const PAUSE: RegressionSpec = RegressionSpec {
    cpu_baseline: 43358,
    mem_baseline: 5217,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const UNPAUSE: RegressionSpec = RegressionSpec {
    cpu_baseline: 69428,
    mem_baseline: 8000,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const CLEAR_EMERGENCY: RegressionSpec = RegressionSpec {
    cpu_baseline: 53721,
    mem_baseline: 6568,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const SCHEDULE_UNPAUSE: RegressionSpec = RegressionSpec {
    cpu_baseline: 49093,
    mem_baseline: 6317,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const TRANSFER_ADMIN: RegressionSpec = RegressionSpec {
    cpu_baseline: 40575,
    mem_baseline: 4936,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const PAUSE_FUNCTION: RegressionSpec = RegressionSpec {
    cpu_baseline: 47263,
    mem_baseline: 5701,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const UNPAUSE_FUNCTION: RegressionSpec = RegressionSpec {
    cpu_baseline: 63556,
    mem_baseline: 7538,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const PAUSE_MODULE: RegressionSpec = RegressionSpec {
    cpu_baseline: 42067,
    mem_baseline: 5238,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const UNPAUSE_MODULE: RegressionSpec = RegressionSpec {
    cpu_baseline: 52384,
    mem_baseline: 6750,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const IS_PAUSED: RegressionSpec = RegressionSpec {
    cpu_baseline: 18092,
    mem_baseline: 2070,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const IS_FUNCTION_PAUSED: RegressionSpec = RegressionSpec {
    cpu_baseline: 38097,
    mem_baseline: 3890,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const IS_MODULE_PAUSED: RegressionSpec = RegressionSpec {
    cpu_baseline: 28118,
    mem_baseline: 3142,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const LIST_PAUSED_FUNCTIONS: RegressionSpec = RegressionSpec {
    cpu_baseline: 32686,
    mem_baseline: 3757,
    cpu_threshold_percent: 10,
    mem_threshold_percent: 10,
};

const GET_UNPAUSE_SCHEDULE: RegressionSpec = RegressionSpec {
    cpu_baseline: 32393,
    mem_baseline: 3702,
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
        "GAS_BENCH_RESULT {{\"contract\":\"emergency_killswitch\",\"method\":\"{}\",\"scenario\":\"{}\",\"cpu\":{},\"mem\":{},\"cpu_baseline\":{},\"mem_baseline\":{},\"cpu_threshold_percent\":{},\"mem_threshold_percent\":{}}}",
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
fn bench_emergency_killswitch_initialize() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    let (cpu, mem, _) = measure(&env, || client.initialize(&admin));
    assert!(!client.is_paused());

    emit_bench_result("initialize", "initialized", cpu, mem, INITIALIZE);
    assert_regression_bounds("initialize", "initialized", cpu, mem, INITIALIZE);
}

#[test]
fn bench_emergency_killswitch_pause() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let (cpu, mem, _) = measure(&env, || client.pause());
    assert!(client.is_paused());

    emit_bench_result("pause", "global_pause", cpu, mem, PAUSE);
    assert_regression_bounds("pause", "global_pause", cpu, mem, PAUSE);
}

#[test]
fn bench_emergency_killswitch_schedule_unpause() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();

    let future = env.ledger().timestamp() + 3600;
    let (cpu, mem, _) = measure(&env, || client.schedule_unpause(&future));
    assert!(client.get_unpause_schedule().is_some());

    emit_bench_result(
        "schedule_unpause",
        "schedule_future_unpause",
        cpu,
        mem,
        SCHEDULE_UNPAUSE,
    );
    assert_regression_bounds(
        "schedule_unpause",
        "schedule_future_unpause",
        cpu,
        mem,
        SCHEDULE_UNPAUSE,
    );
}

#[test]
fn bench_emergency_killswitch_unpause() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    env.ledger().set_timestamp(future);

    let (cpu, mem, _) = measure(&env, || client.unpause());
    assert!(!client.is_paused());

    emit_bench_result("unpause", "timelocked_unpause", cpu, mem, UNPAUSE);
    assert_regression_bounds("unpause", "timelocked_unpause", cpu, mem, UNPAUSE);
}

#[test]
fn bench_emergency_killswitch_clear_emergency_state() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();

    let (cpu, mem, _) = measure(&env, || client.clear_emergency_state());
    assert!(!client.is_paused());

    emit_bench_result(
        "clear_emergency_state",
        "stuck_pause_recovery",
        cpu,
        mem,
        CLEAR_EMERGENCY,
    );
    assert_regression_bounds(
        "clear_emergency_state",
        "stuck_pause_recovery",
        cpu,
        mem,
        CLEAR_EMERGENCY,
    );
}

#[test]
fn bench_emergency_killswitch_transfer_admin() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize(&admin);

    let (cpu, mem, _) = measure(&env, || client.transfer_admin(&new_admin));

    emit_bench_result(
        "transfer_admin",
        "admin_transfer",
        cpu,
        mem,
        TRANSFER_ADMIN,
    );
    assert_regression_bounds(
        "transfer_admin",
        "admin_transfer",
        cpu,
        mem,
        TRANSFER_ADMIN,
    );
}

#[test]
fn bench_emergency_killswitch_pause_function() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");

    let (cpu, mem, _) = measure(&env, || client.pause_function(&module, &func));

    emit_bench_result(
        "pause_function",
        "single_function_pause",
        cpu,
        mem,
        PAUSE_FUNCTION,
    );
    assert_regression_bounds(
        "pause_function",
        "single_function_pause",
        cpu,
        mem,
        PAUSE_FUNCTION,
    );
}

#[test]
fn bench_emergency_killswitch_unpause_function() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");
    client.pause_function(&module, &func);

    let (cpu, mem, _) = measure(&env, || client.unpause_function(&module, &func));

    emit_bench_result(
        "unpause_function",
        "single_function_unpause",
        cpu,
        mem,
        UNPAUSE_FUNCTION,
    );
    assert_regression_bounds(
        "unpause_function",
        "single_function_unpause",
        cpu,
        mem,
        UNPAUSE_FUNCTION,
    );
}

#[test]
fn bench_emergency_killswitch_pause_module() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");

    let (cpu, mem, _) = measure(&env, || client.pause_module(&module));

    emit_bench_result(
        "pause_module",
        "single_module_pause",
        cpu,
        mem,
        PAUSE_MODULE,
    );
    assert_regression_bounds(
        "pause_module",
        "single_module_pause",
        cpu,
        mem,
        PAUSE_MODULE,
    );
}

#[test]
fn bench_emergency_killswitch_unpause_module() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    client.pause_module(&module);

    let (cpu, mem, _) = measure(&env, || client.unpause_module(&module));

    emit_bench_result(
        "unpause_module",
        "single_module_unpause",
        cpu,
        mem,
        UNPAUSE_MODULE,
    );
    assert_regression_bounds(
        "unpause_module",
        "single_module_unpause",
        cpu,
        mem,
        UNPAUSE_MODULE,
    );
}

#[test]
fn bench_emergency_killswitch_is_paused() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let (cpu, mem, paused) = measure(&env, || client.is_paused());
    assert!(!paused);

    emit_bench_result("is_paused", "not_paused", cpu, mem, IS_PAUSED);
    assert_regression_bounds("is_paused", "not_paused", cpu, mem, IS_PAUSED);
}

#[test]
fn bench_emergency_killswitch_is_function_paused() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");
    client.pause_function(&module, &func);

    let (cpu, mem, paused) = measure(&env, || client.is_function_paused(&module, &func));
    assert!(paused);

    emit_bench_result(
        "is_function_paused",
        "function_paused",
        cpu,
        mem,
        IS_FUNCTION_PAUSED,
    );
    assert_regression_bounds(
        "is_function_paused",
        "function_paused",
        cpu,
        mem,
        IS_FUNCTION_PAUSED,
    );
}

#[test]
fn bench_emergency_killswitch_is_module_paused() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    client.pause_module(&module);

    let (cpu, mem, paused) = measure(&env, || client.is_module_paused(&module));
    assert!(paused);

    emit_bench_result(
        "is_module_paused",
        "module_paused",
        cpu,
        mem,
        IS_MODULE_PAUSED,
    );
    assert_regression_bounds(
        "is_module_paused",
        "module_paused",
        cpu,
        mem,
        IS_MODULE_PAUSED,
    );
}

#[test]
fn bench_emergency_killswitch_list_paused_functions() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    client.pause_function(&module, &symbol_short!("pay"));
    client.pause_function(&module, &symbol_short!("refund"));

    let (cpu, mem, list) = measure(&env, || client.list_paused_functions(&module));
    assert_eq!(list.len(), 2);

    emit_bench_result(
        "list_paused_functions",
        "two_functions_paused",
        cpu,
        mem,
        LIST_PAUSED_FUNCTIONS,
    );
    assert_regression_bounds(
        "list_paused_functions",
        "two_functions_paused",
        cpu,
        mem,
        LIST_PAUSED_FUNCTIONS,
    );
}

#[test]
fn bench_emergency_killswitch_get_unpause_schedule() {
    let env = bench_env();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);

    let (cpu, mem, schedule) = measure(&env, || client.get_unpause_schedule());
    assert_eq!(schedule, Some(future));

    emit_bench_result(
        "get_unpause_schedule",
        "schedule_set",
        cpu,
        mem,
        GET_UNPAUSE_SCHEDULE,
    );
    assert_regression_bounds(
        "get_unpause_schedule",
        "schedule_set",
        cpu,
        mem,
        GET_UNPAUSE_SCHEDULE,
    );
}
