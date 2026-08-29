use bill_payments::{
    BillPayments, BillPaymentsClient, Error, CANCEL_BILL_RATE_LIMIT, CREATE_BILL_RATE_LIMIT,
};
use remitwise_common::{
    set_cross_contract_epoch, set_trusted_orchestrator, MAX_BATCH_SIZE,
    RATE_LIMIT_WINDOW_SECONDS,
};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, BytesN, Env, String, Vec};

const CURRENCY_XLM: &str = "XLM";
const FAR_FUTURE_TS: u64 = 2_000_000_000;

/// Baseline and threshold config for a single benchmark scenario.
///
/// CI note:
/// - Keep these values synchronized with `benchmarks/baseline.json` and `benchmarks/thresholds.json`.
/// - Intentionally tight thresholds make regressions fail fast.
#[derive(Clone, Copy)]
struct RegressionSpec {
    cpu_baseline: u64,
    mem_baseline: u64,
    cpu_threshold_percent: u64,
    mem_threshold_percent: u64,
}

// ---------------------------------------------------------------------------
// Regression specs for archiving / restore / cleanup (measured baselines)
// ---------------------------------------------------------------------------

const ARCHIVE_99_PAID: RegressionSpec = RegressionSpec {
    cpu_baseline: 0,
    mem_baseline: 0,
    cpu_threshold_percent: 15,
    mem_threshold_percent: 12,
};

const RESTORE_SINGLE_ARCHIVED: RegressionSpec = RegressionSpec {
    cpu_baseline: 150_000,
    mem_baseline: 26_000,
    cpu_threshold_percent: 12,
    mem_threshold_percent: 10,
};

const CLEANUP_ARCHIVED_MIXED_AGE: RegressionSpec = RegressionSpec {
    cpu_baseline: 1_950_000,
    mem_baseline: 370_000,
    cpu_threshold_percent: 15,
    mem_threshold_percent: 12,
};

const BATCH_PAY_MIXED_50: RegressionSpec = RegressionSpec {
    cpu_baseline: 3_100_000,
    mem_baseline: 700_000,
    cpu_threshold_percent: 15,
    mem_threshold_percent: 12,
};

// ---------------------------------------------------------------------------
// Regression specs for keyed idempotency paths
//
// These are deliberately generous first-pass bounds. Each scenario emits its
// observed cost so the baselines can be tightened after stable CI measurements.
// ---------------------------------------------------------------------------

const KEYED_RECURRING_PAY_FIRST: RegressionSpec = RegressionSpec {
    cpu_baseline: 10_000_000,
    mem_baseline: 2_000_000,
    cpu_threshold_percent: 35,
    mem_threshold_percent: 35,
};

const KEYED_RECURRING_PAY_REPLAY: RegressionSpec = RegressionSpec {
    cpu_baseline: 3_000_000,
    mem_baseline: 750_000,
    cpu_threshold_percent: 35,
    mem_threshold_percent: 35,
};

const KEYED_DUE_SCHEDULE_FIRST: RegressionSpec = RegressionSpec {
    cpu_baseline: 10_000_000,
    mem_baseline: 2_000_000,
    cpu_threshold_percent: 35,
    mem_threshold_percent: 35,
};

const KEYED_DUE_SCHEDULE_REPLAY: RegressionSpec = RegressionSpec {
    cpu_baseline: 3_000_000,
    mem_baseline: 750_000,
    cpu_threshold_percent: 35,
    mem_threshold_percent: 35,
};

// ---------------------------------------------------------------------------
// Regression specs for single-page list queries
//
// Baselines are intentionally generous first-pass approximations: the exact
// values are printed in GAS_BENCH_RESULT lines at runtime so operators can
// tighten them once a stable run has been observed.  Thresholds are set to
// 25 % to catch meaningful regressions without failing on minor variation.
// ---------------------------------------------------------------------------

/// Single page of 50 unpaid bills out of a 50-bill dataset (full scan, last page).
const UNPAID_BILLS_PAGE_50: RegressionSpec = RegressionSpec {
    cpu_baseline: 2_500_000,
    mem_baseline: 500_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

/// First page of 50 unpaid bills out of a 200-bill dataset.
const UNPAID_BILLS_PAGE_200: RegressionSpec = RegressionSpec {
    cpu_baseline: 7_000_000,
    mem_baseline: 1_400_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

/// First page of 50 unpaid bills out of a 1 000-bill dataset.
const UNPAID_BILLS_PAGE_1000: RegressionSpec = RegressionSpec {
    cpu_baseline: 30_000_000,
    mem_baseline: 6_500_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

/// Single page of 50 overdue bills out of a 50-bill dataset.
const OVERDUE_BILLS_PAGE_50: RegressionSpec = RegressionSpec {
    cpu_baseline: 4_500_000,
    mem_baseline: 500_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

/// First page of 50 overdue bills out of a 200-bill dataset.
const OVERDUE_BILLS_PAGE_200: RegressionSpec = RegressionSpec {
    cpu_baseline: 9_500_000,
    mem_baseline: 1_400_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

/// First page of 50 overdue bills out of a 1 000-bill dataset.
const OVERDUE_BILLS_PAGE_1000: RegressionSpec = RegressionSpec {
    cpu_baseline: 35_000_000,
    mem_baseline: 6_500_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

/// Single page of 50 owner bills out of a 50-bill dataset.
const OWNER_BILLS_PAGE_50: RegressionSpec = RegressionSpec {
    cpu_baseline: 2_500_000,
    mem_baseline: 500_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

/// First page of 50 owner bills out of a 200-bill dataset.
const OWNER_BILLS_PAGE_200: RegressionSpec = RegressionSpec {
    cpu_baseline: 7_000_000,
    mem_baseline: 1_400_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

/// First page of 50 owner bills out of a 1 000-bill dataset.
const OWNER_BILLS_PAGE_1000: RegressionSpec = RegressionSpec {
    cpu_baseline: 30_000_000,
    mem_baseline: 6_500_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

// ---------------------------------------------------------------------------
// Regression specs for full multi-page traversal benchmarks
//
// These cover the cursor-walk pattern (calling the list endpoint repeatedly
// until next_cursor == 0).  Cost scales with total bill count so the baselines
// are larger; 25 % threshold still catches real regressions.
// ---------------------------------------------------------------------------

/// Walk all pages of 100 unpaid bills (2 pages × 50).
const UNPAID_MULTIPAGE_100: RegressionSpec = RegressionSpec {
    cpu_baseline: 8_000_000,
    mem_baseline: 1_600_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

/// Walk all pages of 100 overdue bills (2 pages × 50).
const OVERDUE_MULTIPAGE_100: RegressionSpec = RegressionSpec {
    cpu_baseline: 12_000_000,
    mem_baseline: 1_600_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

/// Walk all pages of 100 owner bills (2 pages × 50).
const OWNER_MULTIPAGE_100: RegressionSpec = RegressionSpec {
    cpu_baseline: 8_500_000,
    mem_baseline: 1_600_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

// ---------------------------------------------------------------------------
// Regression specs for get_overdue_bills_for_owner (owner-scoped variant)
// ---------------------------------------------------------------------------

/// Owner-scoped overdue page, 50 total bills all overdue.
const OVERDUE_FOR_OWNER_PAGE_50: RegressionSpec = RegressionSpec {
    cpu_baseline: 3_500_000,
    mem_baseline: 600_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

/// Owner-scoped overdue: first page when owner has 200 overdue bills.
const OVERDUE_FOR_OWNER_PAGE_200: RegressionSpec = RegressionSpec {
    cpu_baseline: 7_000_000,
    mem_baseline: 1_400_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

// ---------------------------------------------------------------------------
// Regression specs for linear-scaling assertions
// ---------------------------------------------------------------------------

/// Per-page cost when dataset has 50 bills (used in scaling guard).
#[allow(dead_code)]
const SCALING_UNPAID_50: RegressionSpec = RegressionSpec {
    cpu_baseline: 2_500_000,
    mem_baseline: 500_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
};

/// Per-page cost when dataset has 100 bills (used in scaling guard).
const SCALING_UNPAID_100: RegressionSpec = RegressionSpec {
    cpu_baseline: 4_000_000,
    mem_baseline: 750_000,
    cpu_threshold_percent: 25,
    mem_threshold_percent: 25,
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

fn set_time(env: &Env, timestamp: u64) {
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: env.ledger().sequence() + 1,
        timestamp,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100_000,
    });
}

fn request_key(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

fn configure_pay_guard(
    env: &Env,
    contract_id: &Address,
    orchestrator: &Address,
    epoch: u64,
) {
    env.as_contract(contract_id, || {
        set_trusted_orchestrator(env, orchestrator);
        set_cross_contract_epoch(env, epoch);
    });
}

/// Cancel bills while respecting per-address cancel rate limits in tests.
fn cancel_many_bills(client: &BillPaymentsClient, env: &Env, owner: &Address, bill_ids: &Vec<u32>) {
    for (i, bill_id) in bill_ids.iter().enumerate() {
        if i > 0 && (i as u32).is_multiple_of(CANCEL_BILL_RATE_LIMIT) {
            set_time(env, env.ledger().timestamp() + RATE_LIMIT_WINDOW_SECONDS);
        }
        client.cancel_bill(owner, &bill_id);
    }
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

fn create_bill(
    client: &BillPaymentsClient,
    env: &Env,
    owner: &Address,
    name: &str,
    amount: i128,
) -> u32 {
    client.create_bill(
        owner,
        &String::from_str(env, name),
        &amount,
        &FAR_FUTURE_TS,
        &false,
        &0u32,
        &None,
        &String::from_str(env, CURRENCY_XLM),
        &None,
    )
}

fn create_many_bills(
    client: &BillPaymentsClient,
    env: &Env,
    owner: &Address,
    prefix: &str,
    count: u32,
    due_date: u64,
) -> Vec<u32> {
    let mut ids = Vec::new(env);
    for i in 0..count {
        if i > 0 && (i as u32).is_multiple_of(CREATE_BILL_RATE_LIMIT) {
            set_time(env, env.ledger().timestamp() + RATE_LIMIT_WINDOW_SECONDS);
        }
        let name = format!("{}-{}", prefix, i);
        let bill_due_date = if due_date == FAR_FUTURE_TS {
            FAR_FUTURE_TS
        } else {
            env.ledger().timestamp() + 100_000
        };
        let id = client.create_bill(
            owner,
            &String::from_str(env, &name),
            &(100 + i as i128),
            &bill_due_date,
            &false,
            &0u32,
            &None,
            &String::from_str(env, CURRENCY_XLM),
            &None,
        );
        ids.push_back(id);
    }
    ids
}

fn create_many_unpaid(
    client: &BillPaymentsClient,
    env: &Env,
    owner: &Address,
    prefix: &str,
    count: u32,
) -> Vec<u32> {
    create_many_bills(client, env, owner, prefix, count, FAR_FUTURE_TS)
}

fn pay_all(client: &BillPaymentsClient, ids: &Vec<u32>, owner: &Address) {
    for id in ids.iter() {
        client.pay_bill(owner, &id);
    }
}

fn create_many_overdue(
    client: &BillPaymentsClient,
    env: &Env,
    owner: &Address,
    prefix: &str,
    count: u32,
) -> Vec<u32> {
    let ids = create_many_bills(client, env, owner, prefix, count, 0);
    set_time(env, env.ledger().timestamp() + 200_000);
    ids
}

#[allow(dead_code)]
fn max_allowed(baseline: u64, threshold_percent: u64) -> u64 {
    baseline + baseline.saturating_mul(threshold_percent) / 100
}

#[allow(dead_code)]
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
    // CI-friendly line with a stable prefix for downstream parsing.
    println!(
        "GAS_BENCH_RESULT {{\"contract\":\"bill_payments\",\"method\":\"{}\",\"scenario\":\"{}\",\"cpu\":{},\"mem\":{},\"cpu_baseline\":{},\"mem_baseline\":{},\"cpu_threshold_percent\":{},\"mem_threshold_percent\":{}}}",
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

/// Benchmark a keyed recurring payment and its exact receipt replay.
///
/// The replay must return the original receipt without paying again or spawning
/// another child bill.
#[test]
fn bench_pay_bill_keyed_recurring_first_and_replay_with_thresholds() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    let orchestrator = <Address as AddressTrait>::generate(&env);
    let epoch = 7u64;
    configure_pay_guard(&env, &contract_id, &orchestrator, epoch);

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Keyed recurring bench"),
        &1_500i128,
        &FAR_FUTURE_TS,
        &true,
        &30u32,
        &None,
        &String::from_str(&env, CURRENCY_XLM),
        &None,
    );
    let key = request_key(&env, 0x51);

    let (first_cpu, first_mem, first_receipt) = measure(&env, || {
        client.pay_bill_keyed(&orchestrator, &epoch, &owner, &key, &bill_id)
    });
    let count_after_first = client.get_owner_bill_count(&owner);
    let total_after_first = client.get_total_unpaid(&owner);
    assert_eq!(count_after_first, 2, "payment must add exactly one child");
    assert_eq!(
        total_after_first, 1_500,
        "recurring child must replace the paid amount in unpaid totals"
    );
    let child_id = first_receipt
        .child_bill_id
        .expect("recurring payment must return a child bill id");
    let child_after_first = client
        .get_bill(&child_id)
        .expect("recurring payment must create its child bill");

    let (replay_cpu, replay_mem, replay_receipt) = measure(&env, || {
        client.pay_bill_keyed(&orchestrator, &epoch, &owner, &key, &bill_id)
    });

    assert_eq!(replay_receipt.bill_id, first_receipt.bill_id);
    assert_eq!(replay_receipt.paid_amount, first_receipt.paid_amount);
    assert_eq!(replay_receipt.child_bill_id, first_receipt.child_bill_id);
    assert_eq!(replay_receipt.child_due_date, first_receipt.child_due_date);
    assert!(client.get_bill(&bill_id).unwrap().paid);
    assert_eq!(client.get_owner_bill_count(&owner), count_after_first);
    assert_eq!(client.get_total_unpaid(&owner), total_after_first);
    let child_after_replay = client.get_bill(&child_id).unwrap();
    assert_eq!(child_after_replay.id, child_after_first.id);
    assert_eq!(child_after_replay.due_date, child_after_first.due_date);
    assert!(
        client.get_bill(&(child_id + 1)).is_none(),
        "receipt replay must not spawn a second child"
    );

    emit_bench_result(
        "pay_bill_keyed",
        "recurring_first_execution",
        first_cpu,
        first_mem,
        KEYED_RECURRING_PAY_FIRST,
    );
    assert_regression_bounds(
        "pay_bill_keyed",
        "recurring_first_execution",
        first_cpu,
        first_mem,
        KEYED_RECURRING_PAY_FIRST,
    );
    emit_bench_result(
        "pay_bill_keyed",
        "recurring_exact_receipt_replay",
        replay_cpu,
        replay_mem,
        KEYED_RECURRING_PAY_REPLAY,
    );
    assert_regression_bounds(
        "pay_bill_keyed",
        "recurring_exact_receipt_replay",
        replay_cpu,
        replay_mem,
        KEYED_RECURRING_PAY_REPLAY,
    );
}

/// Benchmark keyed due-schedule execution and exact replay of its schedule IDs.
///
/// The replay must not mint another bill or advance the schedule a second time.
#[test]
fn bench_execute_due_bill_schedules_keyed_first_and_replay_with_thresholds() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    let executor = <Address as AddressTrait>::generate(&env);
    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Keyed schedule bench"),
        &2_500i128,
        &String::from_str(&env, CURRENCY_XLM),
        &(now + 1_000),
        &86_400u64,
    );
    let key = request_key(&env, 0x52);
    set_time(&env, now + 2_000);

    let (first_cpu, first_mem, first_ids) = measure(&env, || {
        client.execute_due_bill_schedules_keyed(&executor, &key)
    });
    assert_eq!(first_ids.len(), 1);
    assert_eq!(first_ids.get(0).unwrap(), schedule_id);
    let schedule_after_first = client.get_bill_schedule(&schedule_id).unwrap();
    let count_after_first = client.get_owner_bill_count(&owner);
    let total_after_first = client.get_total_unpaid(&owner);
    assert_eq!(
        count_after_first, 1,
        "schedule execution must mint exactly one bill"
    );
    assert_eq!(total_after_first, 2_500);

    let (replay_cpu, replay_mem, replay_ids) = measure(&env, || {
        client.execute_due_bill_schedules_keyed(&executor, &key)
    });

    assert_eq!(replay_ids.len(), first_ids.len());
    assert_eq!(replay_ids.get(0).unwrap(), first_ids.get(0).unwrap());
    assert_eq!(client.get_owner_bill_count(&owner), count_after_first);
    assert_eq!(client.get_total_unpaid(&owner), total_after_first);
    let schedule_after_replay = client.get_bill_schedule(&schedule_id).unwrap();
    assert_eq!(
        schedule_after_replay.last_executed,
        schedule_after_first.last_executed
    );
    assert_eq!(schedule_after_replay.next_due, schedule_after_first.next_due);
    assert_eq!(
        schedule_after_replay.missed_count,
        schedule_after_first.missed_count
    );

    emit_bench_result(
        "execute_due_bill_schedules_keyed",
        "single_due_schedule_first_execution",
        first_cpu,
        first_mem,
        KEYED_DUE_SCHEDULE_FIRST,
    );
    assert_regression_bounds(
        "execute_due_bill_schedules_keyed",
        "single_due_schedule_first_execution",
        first_cpu,
        first_mem,
        KEYED_DUE_SCHEDULE_FIRST,
    );
    emit_bench_result(
        "execute_due_bill_schedules_keyed",
        "single_due_schedule_exact_replay",
        replay_cpu,
        replay_mem,
        KEYED_DUE_SCHEDULE_REPLAY,
    );
    assert_regression_bounds(
        "execute_due_bill_schedules_keyed",
        "single_due_schedule_exact_replay",
        replay_cpu,
        replay_mem,
        KEYED_DUE_SCHEDULE_REPLAY,
    );
}

/// Benchmark archive on a worst-case-ish state where many paid bills are eligible.
///
/// Security assumptions validated:
/// - Only paid bills are archived.
/// - Unpaid bills remain active after archive.
#[test]
fn bench_archive_paid_bills_99_with_thresholds() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    let paid_ids = create_many_unpaid(&client, &env, &owner, "ArchiveBench", 99);
    pay_all(&client, &paid_ids, &owner);

    // Keep one unpaid bill to verify archive filtering behavior.
    let unpaid_id = create_bill(&client, &env, &owner, "KeepUnpaid", 777);

    let (cpu, mem, archived_count) =
        measure(&env, || client.archive_paid_bills(&owner, &FAR_FUTURE_TS));
    assert_eq!(archived_count, 99);
    assert!(client.get_archived_bill(&1).is_some());
    assert!(client.get_bill(&unpaid_id).is_some());
    assert!(!client.get_bill(&unpaid_id).unwrap().paid);

    emit_bench_result(
        "archive_paid_bills",
        "99_paid_1_unpaid_preserved",
        cpu,
        mem,
        ARCHIVE_99_PAID,
    );
}

/// Benchmark restore of a single archived bill.
///
/// Security assumptions validated:
/// - A non-owner cannot restore another user's archived bill.
/// - Successful restore removes the archived record and re-creates a paid bill.
#[test]
fn bench_restore_archived_bill_single_with_thresholds() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    let attacker = <Address as AddressTrait>::generate(&env);

    let target_id = create_bill(&client, &env, &owner, "RestoreBench", 500);
    client.pay_bill(&owner, &target_id);
    assert_eq!(client.archive_paid_bills(&owner, &FAR_FUTURE_TS), 1);
    assert!(client.get_archived_bill(&target_id).is_some());

    let unauthorized = client.try_restore_bill(&attacker, &target_id);
    assert_eq!(unauthorized, Err(Ok(Error::Unauthorized)));

    let (cpu, mem, restore_result) = measure(&env, || client.restore_bill(&owner, &target_id));
    assert_eq!(restore_result, ());
    let restored = client.get_bill(&target_id).unwrap();
    assert!(restored.paid);
    assert!(client.get_archived_bill(&target_id).is_none());

    emit_bench_result(
        "restore_bill",
        "single_archived_owner_restore",
        cpu,
        mem,
        RESTORE_SINGLE_ARCHIVED,
    );
}

/// Benchmark cleanup with mixed archive ages.
///
/// Security assumptions validated:
/// - Cleanup only removes records with `archived_at < before_timestamp`.
/// - Newer archived entries remain intact.
#[test]
fn bench_bulk_cleanup_archived_mixed_age_with_thresholds() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    // Batch 1: older archive entries.
    let older_ids = create_many_unpaid(&client, &env, &owner, "CleanupOlder", 20);
    pay_all(&client, &older_ids, &owner);
    set_time(&env, 1_700_000_100);
    assert_eq!(client.archive_paid_bills(&owner, &FAR_FUTURE_TS), 20);

    // Batch 2: newer archive entries.
    let newer_ids = create_many_unpaid(&client, &env, &owner, "CleanupNewer", 10);
    pay_all(&client, &newer_ids, &owner);
    set_time(&env, 1_700_000_900);
    assert_eq!(client.archive_paid_bills(&owner, &FAR_FUTURE_TS), 10);

    let cleanup_before = 1_700_000_500u64;
    let (cpu, mem, deleted_count) =
        measure(&env, || client.bulk_cleanup_bills(&owner, &cleanup_before));
    assert_eq!(deleted_count, 20);
    assert!(client
        .get_archived_bill(&older_ids.get(0).unwrap())
        .is_none());
    assert!(client
        .get_archived_bill(&newer_ids.get(0).unwrap())
        .is_some());

    emit_bench_result(
        "bulk_cleanup_bills",
        "mixed_age_20_of_30_deleted",
        cpu,
        mem,
        CLEANUP_ARCHIVED_MIXED_AGE,
    );
}

/// Benchmark batch pay partial-success path with mixed valid/invalid IDs.
///
/// Security assumptions validated:
/// - Unauthorized bill IDs are skipped (no cross-owner payments).
/// - Already paid and missing IDs are skipped deterministically.
/// - Valid IDs in the same batch still succeed.
#[test]
fn bench_batch_pay_bills_mixed_50_with_thresholds() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    let other = <Address as AddressTrait>::generate(&env);

    let owner_ids = create_many_unpaid(&client, &env, &owner, "BatchOwner", 35);
    let owner_ids_len = owner_ids.len();
    for idx in 30..owner_ids_len {
        let id = owner_ids.get(idx).unwrap();
        client.pay_bill(&owner, &id);
    }
    let other_ids = create_many_unpaid(&client, &env, &other, "BatchOther", 10);

    let mut batch = Vec::new(&env);
    for idx in 0..30 {
        batch.push_back(owner_ids.get(idx).unwrap());
    }
    for idx in 30..owner_ids_len {
        batch.push_back(owner_ids.get(idx).unwrap());
    }
    for id in other_ids.iter() {
        batch.push_back(id);
    }
    for id in 0..5 {
        batch.push_back(50_000 + id);
    }
    assert_eq!(batch.len(), 50);

    let (cpu, mem, _) = measure(&env, || client.batch_pay_bills(&owner, &batch));

    for idx in 0..30 {
        let id = owner_ids.get(idx).unwrap();
        assert!(client.get_bill(&id).unwrap().paid);
    }

    emit_bench_result(
        "batch_pay_bills",
        "mixed_batch_50_partial_success",
        cpu,
        mem,
        BATCH_PAY_MIXED_50,
    );
}

/// Benchmark first-page unpaid bill pagination at varying dataset sizes.
#[test]
fn bench_get_unpaid_bills_page_first_50_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    create_many_unpaid(&client, &env, &owner, "Unpaid50", 50);

    let (cpu, mem, page) = measure(&env, || client.get_unpaid_bills(&owner, &0u32, &50u32));
    assert_eq!(page.count, 50);
    assert_eq!(page.items.len(), 50);
    assert_eq!(page.next_cursor, 0);

    emit_bench_result(
        "get_unpaid_bills",
        "50_unpaid_bills_page",
        cpu,
        mem,
        UNPAID_BILLS_PAGE_50,
    );
}

#[test]
fn bench_get_unpaid_bills_page_first_200_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    create_many_unpaid(&client, &env, &owner, "Unpaid200", 200);

    let (cpu, mem, page) = measure(&env, || client.get_unpaid_bills(&owner, &0u32, &50u32));
    assert_eq!(page.count, 50);
    assert_eq!(page.items.len(), 50);
    assert!(page.next_cursor > 0, "expected more pages for 200 bills");

    emit_bench_result(
        "get_unpaid_bills",
        "200_unpaid_bills_page",
        cpu,
        mem,
        UNPAID_BILLS_PAGE_200,
    );
}

#[test]
fn bench_get_unpaid_bills_page_first_1000_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    create_many_unpaid(&client, &env, &owner, "Unpaid1000", 1000);

    let (cpu, mem, page) = measure(&env, || client.get_unpaid_bills(&owner, &0u32, &50u32));
    assert_eq!(page.count, 50);
    assert_eq!(page.items.len(), 50);
    assert!(page.next_cursor > 0, "expected more pages for 1000 bills");

    emit_bench_result(
        "get_unpaid_bills",
        "1000_unpaid_bills_page",
        cpu,
        mem,
        UNPAID_BILLS_PAGE_1000,
    );
}

/// Benchmark first-page overdue bill pagination at varying dataset sizes.
#[test]
fn bench_get_overdue_bills_page_first_50_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    create_many_overdue(&client, &env, &owner, "Overdue50", 50);

    let (cpu, mem, page) = measure(&env, || client.get_overdue_bills(&0u32, &50u32));
    assert_eq!(page.count, 50);
    assert_eq!(page.items.len(), 50);
    assert_eq!(page.next_cursor, 0);

    emit_bench_result(
        "get_overdue_bills",
        "50_overdue_bills_page",
        cpu,
        mem,
        OVERDUE_BILLS_PAGE_50,
    );
}

#[test]
fn bench_get_overdue_bills_page_first_200_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    create_many_overdue(&client, &env, &owner, "Overdue200", 200);

    let (cpu, mem, page) = measure(&env, || client.get_overdue_bills(&0u32, &50u32));
    assert_eq!(page.count, 50);
    assert_eq!(page.items.len(), 50);
    assert!(
        page.next_cursor > 0,
        "expected more pages for 200 overdue bills"
    );

    emit_bench_result(
        "get_overdue_bills",
        "200_overdue_bills_page",
        cpu,
        mem,
        OVERDUE_BILLS_PAGE_200,
    );
}

#[test]
fn bench_get_overdue_bills_page_first_1000_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    create_many_overdue(&client, &env, &owner, "Overdue1000", 1000);

    let (cpu, mem, page) = measure(&env, || client.get_overdue_bills(&0u32, &50u32));
    assert_eq!(page.count, 50);
    assert_eq!(page.items.len(), 50);
    assert!(
        page.next_cursor > 0,
        "expected more pages for 1000 overdue bills"
    );

    emit_bench_result(
        "get_overdue_bills",
        "1000_overdue_bills_page",
        cpu,
        mem,
        OVERDUE_BILLS_PAGE_1000,
    );
}

/// Scale guard: `get_overdue_bills` cost must track the active result set, not the
/// global `NEXT_ID` high-water mark.
///
/// We hold the overdue result set fixed (10 bills) and inflate `NEXT_ID` by
/// creating-then-cancelling 80 filler bills. Cancelled bills leave `OWN_IDX`, so the
/// owner-index walk does identical work in both scenarios. With the previous
/// `1..=NEXT_ID` scan, scenario B (`NEXT_ID == 90`) would have cost ~9x scenario A
/// (`NEXT_ID == 10`); the index walk keeps the cost flat.
#[test]
fn scale_get_overdue_bills_independent_of_next_id() {
    // Scenario A: 10 overdue bills, NEXT_ID == 10.
    let env_a = bench_env();
    let id_a = env_a.register_contract(None, BillPayments);
    let client_a = BillPaymentsClient::new(&env_a, &id_a);
    let owner_a = <Address as AddressTrait>::generate(&env_a);
    create_many_overdue(&client_a, &env_a, &owner_a, "OverdueA", 10);
    let (cpu_a, mem_a, page_a) = measure(&env_a, || client_a.get_overdue_bills(&0u32, &50u32));
    assert_eq!(page_a.count, 10);

    // Scenario B: same 10 overdue bills, but NEXT_ID inflated to 90 via create+cancel.
    let env_b = bench_env();
    let id_b = env_b.register_contract(None, BillPayments);
    let client_b = BillPaymentsClient::new(&env_b, &id_b);
    let owner_b = <Address as AddressTrait>::generate(&env_b);
    create_many_overdue(&client_b, &env_b, &owner_b, "OverdueB", 10);
    let filler = create_many_unpaid(&client_b, &env_b, &owner_b, "Filler", 80);
    cancel_many_bills(&client_b, &env_b, &owner_b, &filler);
    let (cpu_b, mem_b, page_b) = measure(&env_b, || client_b.get_overdue_bills(&0u32, &50u32));
    assert_eq!(
        page_b.count, 10,
        "filler bills cancelled: only the 10 overdue bills remain"
    );

    // Allow a small tolerance for incidental differences; the key invariant is that
    // a 9x larger NEXT_ID does NOT translate into a 9x larger query cost.
    assert!(
        cpu_b <= cpu_a + cpu_a / 5,
        "overdue cpu must not scale with NEXT_ID: A(next_id=10)={}, B(next_id=90)={}",
        cpu_a,
        cpu_b
    );
    assert!(
        mem_b <= mem_a + mem_a / 5,
        "overdue mem must not scale with NEXT_ID: A(next_id=10)={}, B(next_id=90)={}",
        mem_a,
        mem_b
    );

    emit_bench_result(
        "get_overdue_bills",
        "next_id_10_vs_90_fixed_10_result",
        cpu_b,
        mem_b,
        OVERDUE_BILLS_PAGE_50,
    );
}

/// Benchmark owner bill listing pagination at varying dataset sizes.
#[test]
fn bench_get_all_bills_for_owner_page_first_50_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    create_many_unpaid(&client, &env, &owner, "Owner50", 50);

    let (cpu, mem, page) = measure(&env, || {
        client.get_all_bills_for_owner(&owner, &0u32, &50u32)
    });
    assert_eq!(page.count, 50);
    assert_eq!(page.items.len(), 50);
    assert_eq!(page.next_cursor, 0);

    emit_bench_result(
        "get_all_bills_for_owner",
        "50_owner_bills_page",
        cpu,
        mem,
        OWNER_BILLS_PAGE_50,
    );
}

#[test]
fn bench_get_all_bills_for_owner_page_first_200_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    create_many_unpaid(&client, &env, &owner, "Owner200", 200);

    let (cpu, mem, page) = measure(&env, || {
        client.get_all_bills_for_owner(&owner, &0u32, &50u32)
    });
    assert_eq!(page.count, 50);
    assert_eq!(page.items.len(), 50);
    assert!(
        page.next_cursor > 0,
        "expected more pages for 200 owner bills"
    );

    emit_bench_result(
        "get_all_bills_for_owner",
        "200_owner_bills_page",
        cpu,
        mem,
        OWNER_BILLS_PAGE_200,
    );
}

#[test]
fn bench_get_all_bills_for_owner_page_first_1000_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    create_many_unpaid(&client, &env, &owner, "Owner1000", 1000);

    let (cpu, mem, page) = measure(&env, || {
        client.get_all_bills_for_owner(&owner, &0u32, &50u32)
    });
    assert_eq!(page.count, 50);
    assert_eq!(page.items.len(), 50);
    assert!(
        page.next_cursor > 0,
        "expected more pages for 1000 owner bills"
    );

    emit_bench_result(
        "get_all_bills_for_owner",
        "1000_owner_bills_page",
        cpu,
        mem,
        OWNER_BILLS_PAGE_1000,
    );
}

/// Edge case and security guard: reject oversized batch requests.
#[test]
fn edge_batch_pay_rejects_oversized_payload() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    let mut ids = Vec::new(&env);
    for i in 0..(MAX_BATCH_SIZE + 1) {
        ids.push_back(i + 1);
    }

    let result = client.try_batch_pay_bills(&owner, &ids);
    assert_eq!(result, Err(Ok(Error::BatchTooLarge)));
}

// ---------------------------------------------------------------------------
// Multi-page traversal benchmarks (cursor walk)
//
// These tests call the list endpoint repeatedly with the cursor returned by
// each page until `next_cursor == 0`, exercising the full pagination loop.
// The measured cost is the *total* CPU/mem across all page fetches.
//
// Security assumptions validated:
// - Cursor increments monotonically; no items are duplicated across pages.
// - The final page sets `next_cursor == 0`, terminating the loop cleanly.
// - Items per page never exceed MAX_PAGE_LIMIT (50).
// ---------------------------------------------------------------------------

/// Walk every page of 100 unpaid bills (2 × 50-item pages) using cursor chaining.
///
/// The total measured cost must not exceed UNPAID_MULTIPAGE_100 thresholds,
/// confirming the per-page cost is O(page_size) rather than O(total_bills).
#[test]
fn bench_get_unpaid_bills_all_pages_100_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    create_many_unpaid(&client, &env, &owner, "UnpaidMP", 100);

    // Accumulate total budget across all page fetches.
    let mut budget = env.budget();
    budget.reset_unlimited();
    budget.reset_tracker();

    let mut cursor: u32 = 0;
    let mut total_items: u32 = 0;
    let mut pages: u32 = 0;
    loop {
        let page = client.get_unpaid_bills(&owner, &cursor, &50u32);
        total_items += page.count;
        pages += 1;
        assert!(
            page.items.len() <= 50,
            "page returned more items than MAX_PAGE_LIMIT"
        );
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();

    assert_eq!(total_items, 100, "cursor walk must visit all 100 items");
    assert_eq!(
        pages, 2,
        "100 bills with limit=50 must yield exactly 2 pages"
    );

    emit_bench_result(
        "get_unpaid_bills",
        "multipage_walk_100_total",
        cpu,
        mem,
        UNPAID_MULTIPAGE_100,
    );
    assert_regression_bounds(
        "get_unpaid_bills",
        "multipage_walk_100_total",
        cpu,
        mem,
        UNPAID_MULTIPAGE_100,
    );
}

/// Walk every page of 100 overdue bills (2 × 50-item pages) using cursor chaining.
///
/// Security assumption: `get_overdue_bills` is public (no owner auth required).
/// The cursor-walk terminates deterministically when `next_cursor == 0`.
#[test]
fn bench_get_overdue_bills_all_pages_100_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    create_many_overdue(&client, &env, &owner, "OverdueMP", 100);

    let mut budget = env.budget();
    budget.reset_unlimited();
    budget.reset_tracker();

    let mut cursor: u32 = 0;
    let mut total_items: u32 = 0;
    let mut pages: u32 = 0;
    loop {
        let page = client.get_overdue_bills(&cursor, &50u32);
        total_items += page.count;
        pages += 1;
        assert!(
            page.items.len() <= 50,
            "page returned more items than MAX_PAGE_LIMIT"
        );
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();

    assert_eq!(
        total_items, 100,
        "cursor walk must visit all 100 overdue bills"
    );
    assert_eq!(
        pages, 2,
        "100 overdue bills with limit=50 must yield exactly 2 pages"
    );

    emit_bench_result(
        "get_overdue_bills",
        "multipage_walk_100_total",
        cpu,
        mem,
        OVERDUE_MULTIPAGE_100,
    );
    assert_regression_bounds(
        "get_overdue_bills",
        "multipage_walk_100_total",
        cpu,
        mem,
        OVERDUE_MULTIPAGE_100,
    );
}

/// Walk every page of 100 owner bills (2 × 50-item pages) using cursor chaining.
///
/// Validates that `get_all_bills_for_owner` cursor semantics are consistent:
/// the union of all pages equals the full bill set with no duplicates.
#[test]
fn bench_get_all_bills_for_owner_all_pages_100_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    create_many_unpaid(&client, &env, &owner, "OwnerMP", 100);

    let mut budget = env.budget();
    budget.reset_unlimited();
    budget.reset_tracker();

    let mut cursor: u32 = 0;
    let mut total_items: u32 = 0;
    let mut pages: u32 = 0;
    let mut seen_ids = Vec::new(&env);
    loop {
        let page = client.get_all_bills_for_owner(&owner, &cursor, &50u32);
        total_items += page.count;
        pages += 1;
        assert!(
            page.items.len() <= 50,
            "page returned more items than MAX_PAGE_LIMIT"
        );
        for bill in page.items.iter() {
            assert!(
                !seen_ids.contains(&bill.id),
                "duplicate bill id {} across pages",
                bill.id
            );
            seen_ids.push_back(bill.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();

    assert_eq!(
        total_items, 100,
        "cursor walk must visit all 100 owner bills"
    );
    assert_eq!(
        pages, 2,
        "100 owner bills with limit=50 must yield exactly 2 pages"
    );

    emit_bench_result(
        "get_all_bills_for_owner",
        "multipage_walk_100_total",
        cpu,
        mem,
        OWNER_MULTIPAGE_100,
    );
    assert_regression_bounds(
        "get_all_bills_for_owner",
        "multipage_walk_100_total",
        cpu,
        mem,
        OWNER_MULTIPAGE_100,
    );
}

// ---------------------------------------------------------------------------
// get_overdue_bills_for_owner benchmarks (owner-scoped variant)
//
// These mirror the global `get_overdue_bills` benchmarks but exercise the
// owner-scoped counterpart.  The owner-scoped path requires `owner.require_auth()`
// and only walks that owner's index, making it strictly cheaper than the global
// query for single-owner use cases.
//
// Security assumptions validated:
// - `owner.require_auth()` prevents cross-owner overdue inspection.
// - A second owner's overdue bills do not appear in the first owner's results.
// ---------------------------------------------------------------------------

/// Benchmark owner-scoped overdue page: 50 bills, all overdue, single page.
///
/// A second owner with 20 overdue bills is also present to confirm isolation:
/// their bills must not appear in the first owner's result.
#[test]
fn bench_get_overdue_bills_for_owner_page_50_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);
    let other = <Address as AddressTrait>::generate(&env);

    create_many_overdue(&client, &env, &owner, "OwnOverdue50", 50);
    // Second owner — their bills must be invisible to `owner`.
    create_many_overdue(&client, &env, &other, "OtherOverdue", 20);

    let (cpu, mem, page) = measure(&env, || {
        client.get_overdue_bills_for_owner(&owner, &0u32, &50u32)
    });
    assert_eq!(
        page.count, 50,
        "must return exactly the owner's 50 overdue bills"
    );
    assert_eq!(page.items.len(), 50);
    assert_eq!(page.next_cursor, 0, "all 50 fit on one page");
    for bill in page.items.iter() {
        assert_eq!(
            bill.owner, owner,
            "result must not leak another owner's bill"
        );
    }

    emit_bench_result(
        "get_overdue_bills_for_owner",
        "50_overdue_owner_page",
        cpu,
        mem,
        OVERDUE_FOR_OWNER_PAGE_50,
    );
    assert_regression_bounds(
        "get_overdue_bills_for_owner",
        "50_overdue_owner_page",
        cpu,
        mem,
        OVERDUE_FOR_OWNER_PAGE_50,
    );
}

/// Benchmark owner-scoped overdue page: 200 bills all overdue, first page of 50.
///
/// Confirms the per-page cost is stable even when the owner has many overdue bills.
#[test]
fn bench_get_overdue_bills_for_owner_page_200_total() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    create_many_overdue(&client, &env, &owner, "OwnOverdue200", 200);

    let (cpu, mem, page) = measure(&env, || {
        client.get_overdue_bills_for_owner(&owner, &0u32, &50u32)
    });
    assert_eq!(page.count, 50, "first page must return 50 items");
    assert_eq!(page.items.len(), 50);
    assert!(
        page.next_cursor > 0,
        "200 overdue bills must produce more than one page"
    );

    emit_bench_result(
        "get_overdue_bills_for_owner",
        "200_overdue_owner_first_page",
        cpu,
        mem,
        OVERDUE_FOR_OWNER_PAGE_200,
    );
    assert_regression_bounds(
        "get_overdue_bills_for_owner",
        "200_overdue_owner_first_page",
        cpu,
        mem,
        OVERDUE_FOR_OWNER_PAGE_200,
    );
}

// ---------------------------------------------------------------------------
// Linear-scaling assertions
//
// These tests verify that a single `get_unpaid_bills` page fetch costs roughly
// the same whether the owner has 50 or 100 bills, confirming that the per-page
// cost tracks the *page size* (bounded by MAX_PAGE_LIMIT = 50) and not the total
// bill count.  The accepted slack is 50 % of the N=50 cost; a pathological
// O(N) implementation would be ~2× more expensive at N=100 and would fail.
//
// The same guard is applied to `get_overdue_bills` and `get_all_bills_for_owner`
// to ensure all three list endpoints share the same complexity characteristic.
// ---------------------------------------------------------------------------

/// Scaling guard: `get_unpaid_bills` first-page cost must not grow linearly with
/// the total bill count.  A dataset twice as large (100 vs 50 bills) must cost
/// at most 50 % more per page fetch.
#[test]
fn scale_get_unpaid_bills_page_cost_sublinear_50_vs_100() {
    // Scenario A: 50 bills, fetching the only page.
    let env_a = bench_env();
    let id_a = env_a.register_contract(None, BillPayments);
    let client_a = BillPaymentsClient::new(&env_a, &id_a);
    let owner_a = <Address as AddressTrait>::generate(&env_a);
    create_many_unpaid(&client_a, &env_a, &owner_a, "ScaleA", 50);
    let (cpu_a, mem_a, page_a) = measure(&env_a, || {
        client_a.get_unpaid_bills(&owner_a, &0u32, &50u32)
    });
    assert_eq!(page_a.count, 50);

    // Scenario B: 100 bills, fetching the first page of 50.
    let env_b = bench_env();
    let id_b = env_b.register_contract(None, BillPayments);
    let client_b = BillPaymentsClient::new(&env_b, &id_b);
    let owner_b = <Address as AddressTrait>::generate(&env_b);
    create_many_unpaid(&client_b, &env_b, &owner_b, "ScaleB", 100);
    let (cpu_b, mem_b, page_b) = measure(&env_b, || {
        client_b.get_unpaid_bills(&owner_b, &0u32, &50u32)
    });
    assert_eq!(page_b.count, 50);

    // Allowed slack: 75 % of the N=50 cost.
    let cpu_slack = cpu_a * 75 / 100;
    let mem_slack = mem_a * 75 / 100;
    assert!(
        cpu_b <= cpu_a + cpu_slack,
        "get_unpaid_bills page cost scales too steeply: \
         N=50 cpu={}, N=100 cpu={} (max allowed={})",
        cpu_a,
        cpu_b,
        cpu_a + cpu_slack,
    );
    assert!(
        mem_b <= mem_a + mem_slack,
        "get_unpaid_bills page mem scales too steeply: \
         N=50 mem={}, N=100 mem={} (max allowed={})",
        mem_a,
        mem_b,
        mem_a + mem_slack,
    );

    emit_bench_result(
        "get_unpaid_bills",
        "scale_50_vs_100_first_page",
        cpu_b,
        mem_b,
        SCALING_UNPAID_100,
    );
}

/// Scaling guard: `get_overdue_bills` first-page cost must not grow linearly with
/// the total bill count across all owners.
#[test]
fn scale_get_overdue_bills_page_cost_sublinear_50_vs_100() {
    let env_a = bench_env();
    let id_a = env_a.register_contract(None, BillPayments);
    let client_a = BillPaymentsClient::new(&env_a, &id_a);
    let owner_a = <Address as AddressTrait>::generate(&env_a);
    create_many_overdue(&client_a, &env_a, &owner_a, "OvScaleA", 50);
    let (cpu_a, mem_a, page_a) = measure(&env_a, || client_a.get_overdue_bills(&0u32, &50u32));
    assert_eq!(page_a.count, 50);

    let env_b = bench_env();
    let id_b = env_b.register_contract(None, BillPayments);
    let client_b = BillPaymentsClient::new(&env_b, &id_b);
    let owner_b = <Address as AddressTrait>::generate(&env_b);
    create_many_overdue(&client_b, &env_b, &owner_b, "OvScaleB", 100);
    let (cpu_b, mem_b, page_b) = measure(&env_b, || client_b.get_overdue_bills(&0u32, &50u32));
    assert_eq!(page_b.count, 50);

    let cpu_slack = cpu_a * 75 / 100;
    let mem_slack = mem_a * 75 / 100;
    assert!(
        cpu_b <= cpu_a + cpu_slack,
        "get_overdue_bills page cost scales too steeply: \
         N=50 cpu={}, N=100 cpu={} (max allowed={})",
        cpu_a,
        cpu_b,
        cpu_a + cpu_slack,
    );
    assert!(
        mem_b <= mem_a + mem_slack,
        "get_overdue_bills page mem scales too steeply: \
         N=50 mem={}, N=100 mem={} (max allowed={})",
        mem_a,
        mem_b,
        mem_a + mem_slack,
    );

    emit_bench_result(
        "get_overdue_bills",
        "scale_50_vs_100_first_page",
        cpu_b,
        mem_b,
        SCALING_UNPAID_100,
    );
}

/// Scaling guard: `get_all_bills_for_owner` first-page cost must not grow linearly
/// with the total bill count for that owner.
#[test]
fn scale_get_all_bills_for_owner_page_cost_sublinear_50_vs_100() {
    let env_a = bench_env();
    let id_a = env_a.register_contract(None, BillPayments);
    let client_a = BillPaymentsClient::new(&env_a, &id_a);
    let owner_a = <Address as AddressTrait>::generate(&env_a);
    create_many_unpaid(&client_a, &env_a, &owner_a, "OwScaleA", 50);
    let (cpu_a, mem_a, page_a) = measure(&env_a, || {
        client_a.get_all_bills_for_owner(&owner_a, &0u32, &50u32)
    });
    assert_eq!(page_a.count, 50);

    let env_b = bench_env();
    let id_b = env_b.register_contract(None, BillPayments);
    let client_b = BillPaymentsClient::new(&env_b, &id_b);
    let owner_b = <Address as AddressTrait>::generate(&env_b);
    create_many_unpaid(&client_b, &env_b, &owner_b, "OwScaleB", 100);
    let (cpu_b, mem_b, page_b) = measure(&env_b, || {
        client_b.get_all_bills_for_owner(&owner_b, &0u32, &50u32)
    });
    assert_eq!(page_b.count, 50);

    let cpu_slack = cpu_a * 75 / 100;
    let mem_slack = mem_a * 75 / 100;
    assert!(
        cpu_b <= cpu_a + cpu_slack,
        "get_all_bills_for_owner page cost scales too steeply: \
         N=50 cpu={}, N=100 cpu={} (max allowed={})",
        cpu_a,
        cpu_b,
        cpu_a + cpu_slack,
    );
    assert!(
        mem_b <= mem_a + mem_slack,
        "get_all_bills_for_owner page mem scales too steeply: \
         N=50 mem={}, N=100 mem={} (max allowed={})",
        mem_a,
        mem_b,
        mem_a + mem_slack,
    );

    emit_bench_result(
        "get_all_bills_for_owner",
        "scale_50_vs_100_first_page",
        cpu_b,
        mem_b,
        SCALING_UNPAID_100,
    );
}
