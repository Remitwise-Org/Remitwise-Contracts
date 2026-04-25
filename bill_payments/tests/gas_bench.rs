use bill_payments::{BillPayments, BillPaymentsClient, Error};
use remitwise_common::MAX_BATCH_SIZE;
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String, Vec};

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

const ARCHIVE_120_PAID: RegressionSpec = RegressionSpec {
    cpu_baseline: 8_900_000,
    mem_baseline: 2_400_000,
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

const UNPAID_BILLS_PAGE_50: RegressionSpec = RegressionSpec {
    cpu_baseline: 0,
    mem_baseline: 0,
    cpu_threshold_percent: 100,
    mem_threshold_percent: 100,
};

const UNPAID_BILLS_PAGE_200: RegressionSpec = RegressionSpec {
    cpu_baseline: 0,
    mem_baseline: 0,
    cpu_threshold_percent: 100,
    mem_threshold_percent: 100,
};

const UNPAID_BILLS_PAGE_1000: RegressionSpec = RegressionSpec {
    cpu_baseline: 0,
    mem_baseline: 0,
    cpu_threshold_percent: 100,
    mem_threshold_percent: 100,
};

const OVERDUE_BILLS_PAGE_50: RegressionSpec = RegressionSpec {
    cpu_baseline: 0,
    mem_baseline: 0,
    cpu_threshold_percent: 100,
    mem_threshold_percent: 100,
};

const OVERDUE_BILLS_PAGE_200: RegressionSpec = RegressionSpec {
    cpu_baseline: 0,
    mem_baseline: 0,
    cpu_threshold_percent: 100,
    mem_threshold_percent: 100,
};

const OVERDUE_BILLS_PAGE_1000: RegressionSpec = RegressionSpec {
    cpu_baseline: 0,
    mem_baseline: 0,
    cpu_threshold_percent: 100,
    mem_threshold_percent: 100,
};

const OWNER_BILLS_PAGE_50: RegressionSpec = RegressionSpec {
    cpu_baseline: 0,
    mem_baseline: 0,
    cpu_threshold_percent: 100,
    mem_threshold_percent: 100,
};

const OWNER_BILLS_PAGE_200: RegressionSpec = RegressionSpec {
    cpu_baseline: 0,
    mem_baseline: 0,
    cpu_threshold_percent: 100,
    mem_threshold_percent: 100,
};

const OWNER_BILLS_PAGE_1000: RegressionSpec = RegressionSpec {
    cpu_baseline: 0,
    mem_baseline: 0,
    cpu_threshold_percent: 100,
    mem_threshold_percent: 100,
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
        let name = format!("{}-{}", prefix, i);
        let id = client.create_bill(
            owner,
            &String::from_str(env, &name),
            &(100 + i as i128),
            &due_date,
            &false,
            &0u32,
            &None,
            &String::from_str(env, CURRENCY_XLM),
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
    let due_date = env.ledger().timestamp() + 1;
    let ids = create_many_bills(client, env, owner, prefix, count, due_date);
    set_time(env, env.ledger().timestamp() + 2);
    ids
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

/// Benchmark archive on a worst-case-ish state where many paid bills are eligible.
///
/// Security assumptions validated:
/// - Only paid bills are archived.
/// - Unpaid bills remain active after archive.
#[test]
fn bench_archive_paid_bills_120_with_thresholds() {
    let env = bench_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    let paid_ids = create_many_unpaid(&client, &env, &owner, "ArchiveBench", 120);
    pay_all(&client, &paid_ids, &owner);

    // Keep one unpaid bill to verify archive filtering behavior.
    let unpaid_id = create_bill(&client, &env, &owner, "KeepUnpaid", 777);

    let (cpu, mem, archived_count) =
        measure(&env, || client.archive_paid_bills(&owner, &FAR_FUTURE_TS));
    assert_eq!(archived_count, 120);
    assert!(client.get_archived_bill(&1).is_some());
    assert!(client.get_bill(&unpaid_id).is_some());
    assert!(!client.get_bill(&unpaid_id).unwrap().paid);

    assert_regression_bounds(
        "archive_paid_bills",
        "120_paid_1_unpaid_preserved",
        cpu,
        mem,
        ARCHIVE_120_PAID,
    );
    emit_bench_result(
        "archive_paid_bills",
        "120_paid_1_unpaid_preserved",
        cpu,
        mem,
        ARCHIVE_120_PAID,
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

    assert_regression_bounds(
        "restore_bill",
        "single_archived_owner_restore",
        cpu,
        mem,
        RESTORE_SINGLE_ARCHIVED,
    );
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

    assert_regression_bounds(
        "bulk_cleanup_bills",
        "mixed_age_20_of_30_deleted",
        cpu,
        mem,
        CLEANUP_ARCHIVED_MIXED_AGE,
    );
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

    let (cpu, mem, paid_count) = measure(&env, || client.batch_pay_bills(&owner, &batch));
    assert_eq!(paid_count, 30);

    for idx in 0..30 {
        let id = owner_ids.get(idx).unwrap();
        assert!(client.get_bill(&id).unwrap().paid);
    }

    assert_regression_bounds(
        "batch_pay_bills",
        "mixed_batch_50_partial_success",
        cpu,
        mem,
        BATCH_PAY_MIXED_50,
    );
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
    assert!(page.next_cursor > 0, "expected more pages for 200 overdue bills");

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
    assert!(page.next_cursor > 0, "expected more pages for 1000 overdue bills");

    emit_bench_result(
        "get_overdue_bills",
        "1000_overdue_bills_page",
        cpu,
        mem,
        OVERDUE_BILLS_PAGE_1000,
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

    let (cpu, mem, page) = measure(&env, || client.get_all_bills_for_owner(&owner, &0u32, &50u32));
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

    let (cpu, mem, page) = measure(&env, || client.get_all_bills_for_owner(&owner, &0u32, &50u32));
    assert_eq!(page.count, 50);
    assert_eq!(page.items.len(), 50);
    assert!(page.next_cursor > 0, "expected more pages for 200 owner bills");

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

    let (cpu, mem, page) = measure(&env, || client.get_all_bills_for_owner(&owner, &0u32, &50u32));
    assert_eq!(page.count, 50);
    assert_eq!(page.items.len(), 50);
    assert!(page.next_cursor > 0, "expected more pages for 1000 owner bills");

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
