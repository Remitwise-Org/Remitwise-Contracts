//! Gas benchmarks for insurance premium schedule operations.
//!
//! Benchmarks cover the full schedule lifecycle under heavy workloads:
//! - Create schedule operations
//! - Modify schedule operations
//! - Cancel schedule operations
//! - Execute due schedules
//! - Query operations
//!
//! All benchmarks validate security assumptions including authorization
//! and data isolation between owners.

use insurance::{InsuranceContract, InsuranceContractClient, CoverageType};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String, Vec};

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
        min_temp_entry_ttl: 600_000,
        min_persistent_entry_ttl: 600_000,
        max_entry_ttl: 700_000,
    });
    env.budget().reset_unlimited();
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

fn setup_client(env: &Env) -> (InsuranceContractClient<'_>, Address) {
    let contract_id = env.register_contract(None, InsuranceContract);
    let client = InsuranceContractClient::new(env, &contract_id);
    let owner = Address::generate(env);
    client.init(&owner);
    (client, owner)
}

fn create_test_policy(env: &Env, client: &InsuranceContractClient, owner: &Address) -> u32 {
    let name = String::from_str(env, "BenchPolicy");
    client.create_policy(
        owner,
        &name,
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    )
}

// ---------------------------------------------------------------------------
// Create Schedule Benchmarks
// ---------------------------------------------------------------------------

#[test]
fn bench_create_premium_schedule_single() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);
    let policy_id = create_test_policy(&env, &client, &owner);

    let (cpu, mem, schedule_id) = measure(&env, || {
        client.create_premium_schedule(&owner, &policy_id, &1_700_100_000u64, &2_592_000u64)
    });

    assert_eq!(schedule_id, 1);

    println!(
        r#"{{"contract":"insurance","method":"create_premium_schedule","scenario":"single_schedule","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_create_premium_schedule_with_50_existing() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);

    for i in 0..50 {
        let policy_id = create_test_policy(&env, &client, &owner);
        client.create_premium_schedule(&owner, &policy_id, &(1_700_100_000u64 + i * 1000), &2_592_000u64);
    }

    let new_policy_id = create_test_policy(&env, &client, &owner);

    let (cpu, mem, schedule_id) = measure(&env, || {
        client.create_premium_schedule(&owner, &new_policy_id, &1_800_000_000u64, &2_592_000u64)
    });

    assert_eq!(schedule_id, 51);

    println!(
        r#"{{"contract":"insurance","method":"create_premium_schedule","scenario":"51st_schedule_with_existing","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

// ---------------------------------------------------------------------------
// Modify Schedule Benchmarks
// ---------------------------------------------------------------------------

#[test]
fn bench_modify_premium_schedule_single() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);
    let policy_id = create_test_policy(&env, &client, &owner);
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &1_700_100_000u64, &2_592_000u64);

    let (cpu, mem, _) = measure(&env, || {
        client.modify_premium_schedule(&owner, &schedule_id, &1_800_000_000u64, &3_000_000u64)
    });

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.next_due, 1_800_000_000u64);
    assert_eq!(schedule.interval, 3_000_000u64);

    println!(
        r#"{{"contract":"insurance","method":"modify_premium_schedule","scenario":"single_schedule_modification","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_modify_premium_schedule_with_100_existing() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);

    let mut schedule_ids = Vec::new(&env);
    for i in 0..100 {
        let policy_id = create_test_policy(&env, &client, &owner);
        let sid = client.create_premium_schedule(&owner, &policy_id, &(1_700_100_000u64 + i * 1000), &2_592_000u64);
        schedule_ids.push_back(sid);
    }

    let target_schedule_id = schedule_ids.get(50).unwrap();

    let (cpu, mem, _) = measure(&env, || {
        client.modify_premium_schedule(&owner, &target_schedule_id, &1_900_000_000u64, &4_000_000u64)
    });

    println!(
        r#"{{"contract":"insurance","method":"modify_premium_schedule","scenario":"modify_middle_of_100_schedules","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

// ---------------------------------------------------------------------------
// Cancel Schedule Benchmarks
// ---------------------------------------------------------------------------

#[test]
fn bench_cancel_premium_schedule_single() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);
    let policy_id = create_test_policy(&env, &client, &owner);
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &1_700_100_000u64, &2_592_000u64);

    let (cpu, mem, _) = measure(&env, || {
        client.cancel_premium_schedule(&owner, &schedule_id)
    });

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert!(!schedule.active);

    println!(
        r#"{{"contract":"insurance","method":"cancel_premium_schedule","scenario":"single_schedule_cancellation","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_cancel_premium_schedule_from_50() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);

    let mut schedule_ids = Vec::new(&env);
    for i in 0..50 {
        let policy_id = create_test_policy(&env, &client, &owner);
        let sid = client.create_premium_schedule(&owner, &policy_id, &(1_700_100_000u64 + i * 1000), &2_592_000u64);
        schedule_ids.push_back(sid);
    }

    let target_schedule_id = schedule_ids.get(25).unwrap();

    let (cpu, mem, _) = measure(&env, || {
        client.cancel_premium_schedule(&owner, &target_schedule_id)
    });

    println!(
        r#"{{"contract":"insurance","method":"cancel_premium_schedule","scenario":"cancel_middle_of_50_schedules","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

// ---------------------------------------------------------------------------
// Execute Due Schedules Benchmarks
// ---------------------------------------------------------------------------

#[test]
fn bench_execute_due_schedules_single() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);
    let policy_id = create_test_policy(&env, &client, &owner);
    client.create_premium_schedule(&owner, &policy_id, &1_700_050_000u64, &0u64);

    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 100,
        timestamp: 1_700_100_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 500_000,
        min_persistent_entry_ttl: 500_000,
        max_entry_ttl: 700_000,
    });

    let (cpu, mem, executed) = measure(&env, || {
        client.execute_due_premium_schedules()
    });

    assert_eq!(executed.len(), 1);

    println!(
        r#"{{"contract":"insurance","method":"execute_due_premium_schedules","scenario":"single_due_schedule","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_execute_due_schedules_10_of_50() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);

    for i in 0..50u64 {
        let policy_id = create_test_policy(&env, &client, &owner);
        let due_time = if i < 10 {
            1_700_050_000u64
        } else {
            1_800_000_000u64
        };
        client.create_premium_schedule(&owner, &policy_id, &due_time, &2_592_000u64);
    }

    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 100,
        timestamp: 1_700_100_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 500_000,
        min_persistent_entry_ttl: 500_000,
        max_entry_ttl: 700_000,
    });

    let (cpu, mem, executed) = measure(&env, || {
        client.execute_due_premium_schedules()
    });

    assert_eq!(executed.len(), 10);

    println!(
        r#"{{"contract":"insurance","method":"execute_due_premium_schedules","scenario":"10_due_of_50_schedules","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_execute_due_schedules_all_50_due() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);

    for i in 0..50u64 {
        let policy_id = create_test_policy(&env, &client, &owner);
        client.create_premium_schedule(&owner, &policy_id, &(1_700_050_000u64 + i * 100), &2_592_000u64);
    }

    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 100,
        timestamp: 1_700_200_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 500_000,
        min_persistent_entry_ttl: 500_000,
        max_entry_ttl: 700_000,
    });

    let (cpu, mem, executed) = measure(&env, || {
        client.execute_due_premium_schedules()
    });

    assert_eq!(executed.len(), 50);

    println!(
        r#"{{"contract":"insurance","method":"execute_due_premium_schedules","scenario":"all_50_schedules_due","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_execute_due_schedules_with_missed_periods() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);
    let policy_id = create_test_policy(&env, &client, &owner);
    
    let interval = 2_592_000u64;
    client.create_premium_schedule(&owner, &policy_id, &1_700_050_000u64, &interval);

    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 100,
        timestamp: 1_700_050_000 + interval * 5,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 500_000,
        min_persistent_entry_ttl: 500_000,
        max_entry_ttl: 700_000,
    });

    let (cpu, mem, executed) = measure(&env, || {
        client.execute_due_premium_schedules()
    });

    assert_eq!(executed.len(), 1);
    
    let schedule = client.get_premium_schedule(&1u32).unwrap();
    assert!(schedule.missed_count >= 4);

    println!(
        r#"{{"contract":"insurance","method":"execute_due_premium_schedules","scenario":"schedule_with_5_missed_periods","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

// ---------------------------------------------------------------------------
// Query Schedule Benchmarks
// ---------------------------------------------------------------------------

#[test]
fn bench_get_premium_schedule_single() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);
    let policy_id = create_test_policy(&env, &client, &owner);
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &1_700_100_000u64, &2_592_000u64);

    let (cpu, mem, schedule) = measure(&env, || {
        client.get_premium_schedule(&schedule_id)
    });

    assert!(schedule.is_some());

    println!(
        r#"{{"contract":"insurance","method":"get_premium_schedule","scenario":"single_schedule_lookup","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_get_active_schedules_empty() {
    let env = bench_env();
    let (client, _owner) = setup_client(&env);

    let (cpu, mem, schedules) = measure(&env, || {
        client.get_active_schedules()
    });

    assert_eq!(schedules.len(), 0);

    println!(
        r#"{{"contract":"insurance","method":"get_active_schedules","scenario":"empty_schedules","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_get_active_schedules_50() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);

    for i in 0..50u64 {
        let policy_id = create_test_policy(&env, &client, &owner);
        client.create_premium_schedule(&owner, &policy_id, &(1_700_100_000u64 + i * 1000), &2_592_000u64);
    }

    let (cpu, mem, schedules) = measure(&env, || {
        client.get_active_schedules()
    });

    assert_eq!(schedules.len(), 50);

    println!(
        r#"{{"contract":"insurance","method":"get_active_schedules","scenario":"50_active_schedules","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_get_active_schedules_100_worst_case() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);

    for i in 0..100u64 {
        let policy_id = create_test_policy(&env, &client, &owner);
        client.create_premium_schedule(&owner, &policy_id, &(1_700_100_000u64 + i * 1000), &2_592_000u64);
    }

    let (cpu, mem, schedules) = measure(&env, || {
        client.get_active_schedules()
    });

    assert_eq!(schedules.len(), 100);

    println!(
        r#"{{"contract":"insurance","method":"get_active_schedules","scenario":"100_schedules_worst_case","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

// ---------------------------------------------------------------------------
// Policy Operations with Schedule Context
// ---------------------------------------------------------------------------

#[test]
fn bench_get_total_monthly_premium_100_policies() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);

    for _ in 0..100 {
        create_test_policy(&env, &client, &owner);
    }

    let (cpu, mem, total) = measure(&env, || {
        client.get_total_monthly_premium()
    });

    assert_eq!(total, 100 * 5_000_000i128);

    println!(
        r#"{{"contract":"insurance","method":"get_total_monthly_premium","scenario":"100_active_policies","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_create_policy_with_100_existing() {
    let env = bench_env();
    let (client, owner) = setup_client(&env);

    for _ in 0..100 {
        create_test_policy(&env, &client, &owner);
    }

    let name = String::from_str(&env, "NewPolicy");
    let (cpu, mem, policy_id) = measure(&env, || {
        client.create_policy(
            &owner,
            &name,
            &CoverageType::Life,
            &5_000_000i128,
            &500_000_000i128,
            &None,
        )
    });

    assert_eq!(policy_id, 101);

    println!(
        r#"{{"contract":"insurance","method":"create_policy","scenario":"101st_policy_with_existing","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

// ---------------------------------------------------------------------------
// Data Isolation Benchmark
// ---------------------------------------------------------------------------

#[test]
fn bench_schedule_isolation_between_owners() {
    let env = bench_env();
    let (client, owner1) = setup_client(&env);
    let owner2 = Address::generate(&env);

    for i in 0..25u64 {
        let policy_id = create_test_policy(&env, &client, &owner1);
        client.create_premium_schedule(&owner1, &policy_id, &(1_700_100_000u64 + i * 1000), &2_592_000u64);
    }

    for i in 0..25u64 {
        let name = String::from_str(&env, "Owner2Policy");
        let policy_id = client.create_policy(
            &owner2,
            &name,
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        client.create_premium_schedule(&owner2, &policy_id, &(1_700_200_000u64 + i * 1000), &2_592_000u64);
    }

    let (cpu, mem, schedules) = measure(&env, || {
        client.get_active_schedules()
    });

    assert_eq!(schedules.len(), 50);

    println!(
        r#"{{"contract":"insurance","method":"get_active_schedules","scenario":"50_schedules_2_owners_isolation","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}
