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

// get_total_monthly_premium – 100 active policies (pre-existing baseline)
const TOTAL_PREMIUM_100: RegressionSpec = RegressionSpec {
    cpu_baseline: 5_000_000,
    mem_baseline: 1_200_000,
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
    cpu_baseline: 22_000_000,
    mem_baseline: 5_500_000,
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
    cpu_baseline: 2_500_000,
    mem_baseline: 600_000,
    cpu_threshold_percent: 15,
    mem_threshold_percent: 12,
};

// pay_premium – worst-case: payment on the last policy of 500
const PAY_PREMIUM_WORST_500: RegressionSpec = RegressionSpec {
    cpu_baseline: 22_000_000,
    mem_baseline: 5_500_000,
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
    for _ in 0..MAX_POLICIES_PER_OWNER {
        client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128, &None);
    }

    let expected_total = MAX_POLICIES_PER_OWNER as i128 * 100i128;
    let (cpu, mem, total) = measure(&env, || client.get_total_monthly_premium(&owner));
    assert_eq!(total, expected_total);

    println!(
        r#"{{"contract":"insurance","method":"get_total_monthly_premium","scenario":"{}_active_policies","cpu":{},"mem":{}}}"#,
        MAX_POLICIES_PER_OWNER, cpu, mem
    );

    assert_regression_bounds("pay_premium", "typical_n50_first_policy", cpu, mem, PAY_PREMIUM_TYPICAL_50);
    emit_bench_result("pay_premium", "typical_n50_first_policy", cpu, mem, PAY_PREMIUM_TYPICAL_50);
}

/// pay_premium worst-case: 500 existing policies, paying the last one.
///
/// The contract loads the full policy map from storage on every call, so the
/// last-inserted policy represents the maximum storage read cost.
///
/// Validates:
/// - Return value is true.
/// - next_payment_date is updated on the correct policy.
#[test]
fn bench_pay_premium_worst_case_500() {
    let env = bench_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    client.set_pause_admin(&owner, &owner);

    let last_id = seed_policies(&client, &env, &owner, 500);

    let (cpu, mem, ok) = measure(&env, || client.pay_premium(&owner, &last_id));
    assert!(ok, "pay_premium must succeed for the last active policy");

    let policy = client.get_policy(&last_id).expect("policy must exist");
    assert!(
        policy.next_payment_date > 1_700_000_000,
        "next_payment_date must be updated"
    );

    assert_regression_bounds("pay_premium", "worst_case_n500_last_policy", cpu, mem, PAY_PREMIUM_WORST_500);
    emit_bench_result("pay_premium", "worst_case_n500_last_policy", cpu, mem, PAY_PREMIUM_WORST_500);
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
    client.set_pause_admin(&owner, &owner);

    seed_policies(&client, &env, &owner, 10);
    let target_id = 1u32;

    // Attacker attempts to pay the owner's premium – must return false (not panic).
    let result = client.pay_premium(&attacker, &target_id);
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
    client.set_pause_admin(&owner, &owner);

    seed_policies(&client, &env, &owner, 5);
    let target_id = 1u32;
    client.deactivate_policy(&owner, &target_id);

    let result = client.pay_premium(&owner, &target_id);
    assert!(!result, "pay_premium on inactive policy must return false");
}
