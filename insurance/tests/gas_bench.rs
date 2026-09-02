use insurance::{Insurance, InsuranceClient, MAX_POLICIES_PER_OWNER};
use remitwise_common::CoverageType;
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String};

// ---------------------------------------------------------------------------
// Regression specs
// Each spec captures a CPU/memory baseline and an acceptable overshoot (%).
// Tighten baselines after a confirmed optimisation; loosen only with a
// documented justification.
// ---------------------------------------------------------------------------

/// Baseline and threshold config for a single benchmark scenario.
///
/// CI note:
/// - Keep these values synchronised with `benchmarks/baseline.json` and
///   `benchmarks/thresholds.json`.
/// - Intentionally tight thresholds make regressions fail fast.
#[derive(Clone, Copy)]
struct RegressionSpec {
    cpu_baseline: u64,
    mem_baseline: u64,
    cpu_threshold_percent: u64,
    mem_threshold_percent: u64,
}

// get_total_monthly_premium – max active policies per owner.
const TOTAL_PREMIUM_MAX_ACTIVE: RegressionSpec = RegressionSpec {
    cpu_baseline: 2_769_616,
    mem_baseline: 592_934,
    cpu_threshold_percent: 15,
    mem_threshold_percent: 12,
};

// get_active_policies – first page (cursor=0, limit=20) over N policies
const PAGING_FIRST_PAGE_50: RegressionSpec = RegressionSpec {
    cpu_baseline: 2_500_000,
    mem_baseline: 600_000,
    cpu_threshold_percent: 15,
    mem_threshold_percent: 12,
};

const PAGING_FIRST_PAGE_200: RegressionSpec = RegressionSpec {
    cpu_baseline: 9_000_000,
    mem_baseline: 2_200_000,
    cpu_threshold_percent: 15,
    mem_threshold_percent: 12,
};

const PAGING_FIRST_PAGE_500: RegressionSpec = RegressionSpec {
    cpu_baseline: 9_500_000,
    mem_baseline: 2_300_000,
    cpu_threshold_percent: 15,
    mem_threshold_percent: 12,
};

// get_active_policies – last page (worst-case: cursor near end) over N policies
const PAGING_LAST_PAGE_200: RegressionSpec = RegressionSpec {
    cpu_baseline: 9_500_000,
    mem_baseline: 2_300_000,
    cpu_threshold_percent: 15,
    mem_threshold_percent: 12,
};

const PAGING_LAST_PAGE_500: RegressionSpec = RegressionSpec {
    cpu_baseline: 23_000_000,
    mem_baseline: 5_700_000,
    cpu_threshold_percent: 15,
    mem_threshold_percent: 12,
};

// pay_premium – single payment under typical load (50 existing policies)
const PAY_PREMIUM_TYPICAL_50: RegressionSpec = RegressionSpec {
    cpu_baseline: 2_600_000,
    mem_baseline: 620_000,
    cpu_threshold_percent: 15,
    mem_threshold_percent: 15,
};

// pay_premium – worst-case: payment on the last policy of MAX_POLICIES_PER_OWNER
const PAY_PREMIUM_WORST_200: RegressionSpec = RegressionSpec {
    cpu_baseline: 9_000_000,
    mem_baseline: 2_200_000,
    cpu_threshold_percent: 15,
    mem_threshold_percent: 12,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn seed_policies(client: &InsuranceClient, env: &Env, owner: &Address, count: u32) -> u32 {
    let name = String::from_str(env, "BenchPolicy");
    let coverage_type = CoverageType::Health;
    let mut last_id = 0u32;
    for _ in 0..count {
        last_id = client.create_policy(owner, &name, &coverage_type, &100i128, &10_000i128, &None);
    }
    last_id
}

/// Benchmark get_total_monthly_premium at the maximum allowed active-policy count.
#[test]
fn bench_get_total_monthly_premium_worst_case() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    client.set_pause_admin(&owner, &owner);

    let name = String::from_str(&env, "BenchPolicy");
    let coverage_type = CoverageType::Health;
    client.init(&owner);
    for _ in 0..MAX_POLICIES_PER_OWNER {
        client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128, &None);
    }

    let expected_total = MAX_POLICIES_PER_OWNER as i128 * 100i128;
    let (cpu, mem, total) = measure(&env, || client.get_total_monthly_premium(&owner));
    assert_eq!(total, expected_total);

    emit_bench_result(
        "get_total_monthly_premium",
        "max_active_policies",
        cpu,
        mem,
        TOTAL_PREMIUM_MAX_ACTIVE,
    );
}

/// pay_premium worst-case: MAX_POLICIES_PER_OWNER existing policies, paying the last one.
///
/// The contract loads the full policy map from storage on every call, so the
/// last-inserted policy represents the maximum storage read cost.
///
/// Validates:
/// - Return value is true.
/// - next_payment_date is updated on the correct policy.
#[test]
fn bench_pay_premium_worst_case_200() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    client.init(&owner);
    client.set_pause_admin(&owner, &owner);

    let last_id = seed_policies(&client, &env, &owner, MAX_POLICIES_PER_OWNER);
    let orch = <Address as AddressTrait>::generate(&env);
    env.as_contract(&contract_id, || {
        remitwise_common::set_trusted_orchestrator(&env, &orch);
    });

    let (cpu, mem, ok) = measure(&env, || client.pay_premium(&orch, &0, &owner, &last_id));
    assert!(ok, "pay_premium must succeed for the last active policy");

    let policy = client.get_policy(&last_id).expect("policy must exist");
    assert!(
        policy.next_payment_date > 1_700_000_000,
        "next_payment_date must be updated"
    );

    assert_regression_bounds(
        "pay_premium",
        "worst_case_max_policies_last_policy",
        cpu,
        mem,
        PAY_PREMIUM_WORST_200,
    );
    emit_bench_result(
        "pay_premium",
        "worst_case_max_policies_last_policy",
        cpu,
        mem,
        PAY_PREMIUM_WORST_200,
    );
}

/// pay_premium security guard: non-owner cannot pay another owner's premium.
///
/// This is a correctness/security test, not a performance test.
/// Included here so the gas bench suite also covers the auth rejection path.
#[test]
fn bench_pay_premium_rejects_non_owner() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    let attacker = <Address as AddressTrait>::generate(&env);
    client.init(&owner);
    client.set_pause_admin(&owner, &owner);

    seed_policies(&client, &env, &owner, 10);
    let target_id = 1u32;
    let orch = <Address as AddressTrait>::generate(&env);
    env.as_contract(&contract_id, || {
        remitwise_common::set_trusted_orchestrator(&env, &orch);
    });

    // Attacker attempts to pay the owner's premium – must return false (not panic).
    let result = client.pay_premium(&orch, &0, &attacker, &target_id);
    assert!(!result, "non-owner pay_premium must be rejected");

    // Original policy must be unchanged.
    let policy = client.get_policy(&target_id).expect("policy must exist");
    assert_eq!(
        policy.next_payment_date,
        1_700_000_000 + 30 * 86_400,
        "next_payment_date must not change after rejected payment"
    );
}

/// pay_premium on a deactivated policy must return false.
#[test]
fn bench_pay_premium_rejects_inactive_policy() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    client.init(&owner);
    client.set_pause_admin(&owner, &owner);

    seed_policies(&client, &env, &owner, 5);
    let target_id = 1u32;
    client.deactivate_policy(&owner, &target_id);
    let orch = <Address as AddressTrait>::generate(&env);
    env.as_contract(&contract_id, || {
        remitwise_common::set_trusted_orchestrator(&env, &orch);
    });

    let result = client.pay_premium(&orch, &0, &owner, &target_id);
    assert!(!result, "pay_premium on inactive policy must return false");
}

/// Paging benchmark: first page (cursor=0, limit=20) over 50 policies.
#[test]
fn bench_paging_first_page_50() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    client.init(&owner);
    client.set_pause_admin(&owner, &owner);

    seed_policies(&client, &env, &owner, 50);

    let (cpu, mem, page) = measure(&env, || client.get_active_policies(&owner, &0u32, &20u32));
    assert_eq!(page.count, 20, "first page must return 20 policies");
    assert!(
        page.next_cursor > 0,
        "next_cursor must be set for pagination"
    );

    assert_regression_bounds(
        "get_active_policies",
        "first_page_n50",
        cpu,
        mem,
        PAGING_FIRST_PAGE_50,
    );
    emit_bench_result(
        "get_active_policies",
        "first_page_n50",
        cpu,
        mem,
        PAGING_FIRST_PAGE_50,
    );
}

/// Paging benchmark: first page (cursor=0, limit=20) over 200 policies.
#[test]
fn bench_paging_first_page_200() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    client.init(&owner);
    client.set_pause_admin(&owner, &owner);

    seed_policies(&client, &env, &owner, MAX_POLICIES_PER_OWNER);

    let (cpu, mem, page) = measure(&env, || client.get_active_policies(&owner, &0u32, &20u32));
    assert_eq!(page.count, 20, "first page must return 20 policies");
    assert!(
        page.next_cursor > 0,
        "next_cursor must be set for pagination"
    );

    assert_regression_bounds(
        "get_active_policies",
        "first_page_n200",
        cpu,
        mem,
        PAGING_FIRST_PAGE_200,
    );
    emit_bench_result(
        "get_active_policies",
        "first_page_n200",
        cpu,
        mem,
        PAGING_FIRST_PAGE_200,
    );
}

/// Paging benchmark: first page (cursor=0, limit=20) over 500 policies (exceeds MAX_POLICIES_PER_OWNER).
/// This test uses MAX_POLICIES_PER_OWNER since the contract enforces a cap.
#[test]
fn bench_paging_first_page_500() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    client.init(&owner);
    client.set_pause_admin(&owner, &owner);

    // Use MAX_POLICIES_PER_OWNER (200) since contract caps at this value
    seed_policies(&client, &env, &owner, MAX_POLICIES_PER_OWNER);

    let (cpu, mem, page) = measure(&env, || client.get_active_policies(&owner, &0u32, &20u32));
    assert_eq!(page.count, 20, "first page must return 20 policies");
    assert!(
        page.next_cursor > 0,
        "next_cursor must be set for pagination"
    );

    assert_regression_bounds(
        "get_active_policies",
        "first_page_n500",
        cpu,
        mem,
        PAGING_FIRST_PAGE_500,
    );
    emit_bench_result(
        "get_active_policies",
        "first_page_n500",
        cpu,
        mem,
        PAGING_FIRST_PAGE_500,
    );
}

/// Paging benchmark: last page (worst-case cursor near end) over 200 policies.
#[test]
fn bench_paging_last_page_200() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    client.init(&owner);
    client.set_pause_admin(&owner, &owner);

    seed_policies(&client, &env, &owner, MAX_POLICIES_PER_OWNER);

    // Simulate worst-case: cursor near the end (policy ID 80)
    let (cpu, mem, page) = measure(&env, || client.get_active_policies(&owner, &80u32, &20u32));
    assert!(page.count > 0, "last page must return at least one policy");

    assert_regression_bounds(
        "get_active_policies",
        "last_page_n200",
        cpu,
        mem,
        PAGING_LAST_PAGE_200,
    );
    emit_bench_result(
        "get_active_policies",
        "last_page_n200",
        cpu,
        mem,
        PAGING_LAST_PAGE_200,
    );
}

/// Paging benchmark: last page (worst-case cursor near end) over 500 policies (capped at MAX).
#[test]
fn bench_paging_last_page_500() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    client.init(&owner);
    client.set_pause_admin(&owner, &owner);

    // Use MAX_POLICIES_PER_OWNER since contract caps at this value
    seed_policies(&client, &env, &owner, MAX_POLICIES_PER_OWNER);

    // Simulate worst-case: cursor near the end (policy ID 80)
    let (cpu, mem, page) = measure(&env, || client.get_active_policies(&owner, &80u32, &20u32));
    assert!(page.count > 0, "last page must return at least one policy");

    assert_regression_bounds(
        "get_active_policies",
        "last_page_n500",
        cpu,
        mem,
        PAGING_LAST_PAGE_500,
    );
    emit_bench_result(
        "get_active_policies",
        "last_page_n500",
        cpu,
        mem,
        PAGING_LAST_PAGE_500,
    );
}

/// pay_premium typical case: 50 existing policies, paying the first one.
#[test]
fn bench_pay_premium_typical_50() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    client.init(&owner);
    client.set_pause_admin(&owner, &owner);

    seed_policies(&client, &env, &owner, 50);
    let target_id = 1u32;
    let orch = <Address as AddressTrait>::generate(&env);
    env.as_contract(&contract_id, || {
        remitwise_common::set_trusted_orchestrator(&env, &orch);
    });

    let (cpu, mem, ok) = measure(&env, || client.pay_premium(&orch, &0, &owner, &target_id));
    assert!(ok, "pay_premium must succeed for active policy");

    let policy = client.get_policy(&target_id).expect("policy must exist");
    assert!(
        policy.next_payment_date > 1_700_000_000,
        "next_payment_date must be updated"
    );

    assert_regression_bounds(
        "pay_premium",
        "typical_n50_first_policy",
        cpu,
        mem,
        PAY_PREMIUM_TYPICAL_50,
    );
    emit_bench_result(
        "pay_premium",
        "typical_n50_first_policy",
        cpu,
        mem,
        PAY_PREMIUM_TYPICAL_50,
    );
}

fn max_allowed(baseline: u64, threshold_percent: u64) -> u64 {
    baseline + baseline.saturating_mul(threshold_percent) / 100
}

fn assert_regression_bounds(
    method: &str,
    scenario: &str,
    cpu: u64,
    mem: u64,
    spec: RegressionSpec,
) {
    let cpu_max = max_allowed(spec.cpu_baseline, spec.cpu_threshold_percent);
    let mem_max = max_allowed(spec.mem_baseline, spec.mem_threshold_percent);
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

fn emit_bench_result(method: &str, scenario: &str, cpu: u64, mem: u64, spec: RegressionSpec) {
    println!(
        "GAS_BENCH_RESULT {{\"contract\":\"insurance\",\"method\":\"{}\",\"scenario\":\"{}\",\"cpu\":{},\"mem\":{},\"cpu_baseline\":{},\"mem_baseline\":{},\"cpu_threshold_percent\":{},\"mem_threshold_percent\":{}}}",
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
