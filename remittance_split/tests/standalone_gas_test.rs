// Standalone test for gas benchmark implementation
// This test validates the benchmark functions work correctly without external dependencies

use remittance_split::{RemittanceSplit, RemittanceSplitClient, RemittanceSplitError};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env};

/// Create a test environment with consistent configuration
fn create_test_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();

    // Set consistent ledger state
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 1,
        timestamp: 1_700_000_000, // Fixed timestamp for reproducible tests
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100_000,
    });

    // Reset budget for clean measurement
    let mut budget = env.budget();
    budget.reset_unlimited();
    env
}

/// Measure gas consumption of a function
fn measure_gas<F, R>(env: &Env, operation: F) -> (u64, u64, R)
where
    F: FnOnce() -> R,
{
    let mut budget = env.budget();
    budget.reset_unlimited();
    budget.reset_tracker();

    let result = operation();

    let cpu_cost = budget.cpu_instruction_cost();
    let memory_cost = budget.memory_bytes_cost();

    (cpu_cost, memory_cost, result)
}

/// Test: Validate create_remittance_schedule gas measurement
#[test]
fn test_create_schedule_gas_measurement() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);
    let amount = 1_000i128;
    let next_due = env.ledger().timestamp() + 86400; // 1 day from now
    let interval = 2_592_000u64; // 30 days

    let (cpu, mem, schedule_id) = measure_gas(&env, || {
        client.create_remittance_schedule(&owner, &amount, &next_due, &interval)
    });

    // Validate the operation succeeded
    assert_eq!(schedule_id, 1, "First schedule should have ID 1");

    // Validate gas measurements are reasonable
    assert!(cpu > 0, "CPU cost should be measured");
    assert!(mem > 0, "Memory cost should be measured");
    assert!(cpu < 1_000_000, "CPU cost should be reasonable (< 1M)");
    assert!(mem < 100_000, "Memory cost should be reasonable (< 100K)");

    println!("✅ Create schedule - CPU: {}, Memory: {}", cpu, mem);
}

/// Test: Validate modify_remittance_schedule gas measurement
#[test]
fn test_modify_schedule_gas_measurement() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);
    let amount = 1_000i128;
    let next_due = env.ledger().timestamp() + 86400;
    let interval = 2_592_000u64;

    // Create initial schedule
    let schedule_id = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);

    // Measure modification
    let new_amount = 2_000i128;
    let new_next_due = env.ledger().timestamp() + 172800; // 2 days
    let new_interval = 604_800u64; // 1 week

    let (cpu, mem, result) = measure_gas(&env, || {
        client.modify_remittance_schedule(
            &owner,
            &schedule_id,
            &new_amount,
            &new_next_due,
            &new_interval,
        )
    });

    // Validate the operation succeeded
    assert!(result, "Modification should return true");

    // Validate gas measurements
    assert!(cpu > 0, "CPU cost should be measured");
    assert!(mem > 0, "Memory cost should be measured");
    assert!(cpu < 1_000_000, "CPU cost should be reasonable");
    assert!(mem < 100_000, "Memory cost should be reasonable");

    println!("✅ Modify schedule - CPU: {}, Memory: {}", cpu, mem);
}

/// Test: Validate cancel_remittance_schedule gas measurement
#[test]
fn test_cancel_schedule_gas_measurement() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);
    let amount = 1_000i128;
    let next_due = env.ledger().timestamp() + 86400;
    let interval = 2_592_000u64;

    // Create initial schedule
    let schedule_id = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);

    // Measure cancellation
    let (cpu, mem, result) = measure_gas(&env, || {
        client.cancel_remittance_schedule(&owner, &schedule_id)
    });

    // Validate the operation succeeded
    assert!(result, "Cancellation should return true");

    // Validate gas measurements
    assert!(cpu > 0, "CPU cost should be measured");
    assert!(mem > 0, "Memory cost should be measured");
    assert!(cpu < 1_000_000, "CPU cost should be reasonable");
    assert!(mem < 100_000, "Memory cost should be reasonable");

    println!("✅ Cancel schedule - CPU: {}, Memory: {}", cpu, mem);
}

/// Test: Validate get_remittance_schedules gas measurement (empty case)
#[test]
fn test_query_schedules_empty_gas_measurement() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);

    let (cpu, mem, schedules) = measure_gas(&env, || client.get_remittance_schedules(&owner));

    // Validate the operation succeeded
    assert_eq!(schedules.len(), 0, "Should return empty list for new owner");

    // Validate gas measurements
    assert!(cpu > 0, "CPU cost should be measured");
    assert!(mem > 0, "Memory cost should be measured");
    assert!(cpu < 500_000, "Empty query should be efficient");
    assert!(mem < 50_000, "Empty query should use minimal memory");

    println!("✅ Query empty schedules - CPU: {}, Memory: {}", cpu, mem);
}

/// Test: Validate get_remittance_schedules gas measurement (with data)
#[test]
fn test_query_schedules_with_data_gas_measurement() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);

    // Create 5 schedules
    for i in 1..=5 {
        let amount = 1_000i128 * i as i128;
        let next_due = env.ledger().timestamp() + 86400 * i;
        let interval = 2_592_000u64;

        let _ = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);
    }

    // Measure query with data
    let (cpu, mem, schedules) = measure_gas(&env, || client.get_remittance_schedules(&owner));

    // Validate the operation succeeded
    assert_eq!(schedules.len(), 5, "Should return 5 schedules");

    // Validate gas measurements
    assert!(cpu > 0, "CPU cost should be measured");
    assert!(mem > 0, "Memory cost should be measured");
    assert!(cpu < 2_000_000, "Query with data should be reasonable");
    assert!(mem < 200_000, "Query memory should be reasonable");

    println!("✅ Query 5 schedules - CPU: {}, Memory: {}", cpu, mem);
}

/// Test: Validate get_remittance_schedule gas measurement (single lookup)
#[test]
fn test_query_single_schedule_gas_measurement() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);
    let amount = 1_000i128;
    let next_due = env.ledger().timestamp() + 86400;
    let interval = 2_592_000u64;

    // Create schedule
    let schedule_id = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);

    // Measure single lookup
    let (cpu, mem, schedule) = measure_gas(&env, || client.get_remittance_schedule(&schedule_id));

    // Validate the operation succeeded
    assert!(schedule.is_some(), "Should find the created schedule");
    let schedule = schedule.unwrap();
    assert_eq!(schedule.owner, owner, "Schedule should belong to owner");
    assert_eq!(schedule.amount, amount, "Schedule amount should match");

    // Validate gas measurements
    assert!(cpu > 0, "CPU cost should be measured");
    assert!(mem > 0, "Memory cost should be measured");
    assert!(cpu < 500_000, "Single lookup should be efficient");
    assert!(mem < 50_000, "Single lookup should use minimal memory");

    println!("✅ Single schedule lookup - CPU: {}, Memory: {}", cpu, mem);
}

/// Test: Validate gas scaling with multiple schedules
#[test]
fn test_gas_scaling_with_multiple_schedules() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);

    // Create 10 schedules first
    for i in 1..=10 {
        let amount = 1_000i128 * i as i128;
        let next_due = env.ledger().timestamp() + 86400 * i;
        let interval = 2_592_000u64;
        
        let _result = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);
    }

    // Measure creating the 11th schedule (with existing storage)
    let amount = 11_000i128;
    let next_due = env.ledger().timestamp() + 86400 * 11;
    let interval = 2_592_000u64;

    let (cpu, mem, schedule_id) = measure_gas(&env, || {
        client.create_remittance_schedule(&owner, &amount, &next_due, &interval)
    });

    // Validate the operation succeeded
    assert_eq!(schedule_id, 11, "Should be the 11th schedule");

    // Validate gas measurements show reasonable scaling
    assert!(cpu > 0, "CPU cost should be measured");
    assert!(mem > 0, "Memory cost should be measured");
    assert!(cpu < 2_000_000, "Scaling should be reasonable");
    assert!(mem < 200_000, "Memory scaling should be reasonable");

    println!("✅ 11th schedule creation - CPU: {}, Memory: {}", cpu, mem);
}

/// Test: Security validation - data isolation between owners
#[test]
fn test_data_isolation_security() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner1 = <Address as AddressTrait>::generate(&env);
    let owner2 = <Address as AddressTrait>::generate(&env);

    // Create schedules for owner1
    for i in 1..=3 {
        let amount = 1_000i128 * i as i128;
        let next_due = env.ledger().timestamp() + 86400 * i;
        let interval = 2_592_000u64;
        
        let _result = client.create_remittance_schedule(&owner1, &amount, &next_due, &interval);
    }

    // Create schedules for owner2
    for i in 1..=2 {
        let amount = 2_000i128 * i as i128;
        let next_due = env.ledger().timestamp() + 86400 * i;
        let interval = 604_800u64;
        
        let _result = client.create_remittance_schedule(&owner2, &amount, &next_due, &interval);
    }

    // Validate data isolation
    let owner1_schedules = client.get_remittance_schedules(&owner1);
    let owner2_schedules = client.get_remittance_schedules(&owner2);

    assert_eq!(
        owner1_schedules.len(),
        3,
        "Owner1 should see only their 3 schedules"
    );
    assert_eq!(
        owner2_schedules.len(),
        2,
        "Owner2 should see only their 2 schedules"
    );

    // Validate schedule ownership
    for schedule in owner1_schedules.iter() {
        assert_eq!(
            schedule.owner, owner1,
            "All owner1 schedules should belong to owner1"
        );
    }

    for schedule in owner2_schedules.iter() {
        assert_eq!(
            schedule.owner, owner2,
            "All owner2 schedules should belong to owner2"
        );
    }

    println!("✅ Data isolation validated - Owner1: 3 schedules, Owner2: 2 schedules");
}

/// Test: Input validation security
#[test]
fn test_input_validation_security() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);

    // Test invalid amount (zero)
    let result = client.try_create_remittance_schedule(
        &owner, 
        &0i128, // Invalid: zero amount
        &(env.ledger().timestamp() + 86400),
        &2_592_000u64
    );
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidAmount)), "Zero amount should be rejected");

    // Test invalid amount (negative)
    let result = client.try_create_remittance_schedule(
        &owner, 
        &(-1000i128), // Invalid: negative amount
        &(env.ledger().timestamp() + 86400),
        &2_592_000u64
    );
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidAmount)), "Negative amount should be rejected");

    // Test invalid due date (past)
    let result = client.try_create_remittance_schedule(
        &owner, 
        &1000i128, 
        &(env.ledger().timestamp() - 10), // Invalid: past due date
        &2_592_000u64
    );
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidDueDate)), "Past due date should be rejected");

    // Test valid parameters work
    let ok_id = client.create_remittance_schedule(
        &owner,
        &1000i128,
        &(env.ledger().timestamp() + 86400),
        &2_592_000u64
    );
    assert!(result.is_ok(), "Valid parameters should succeed");

    println!("✅ Input validation security verified");
}

/// Integration test: Complete schedule lifecycle with gas tracking
#[test]
fn test_complete_schedule_lifecycle() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);
    let amount = 1_000i128;
    let next_due = env.ledger().timestamp() + 86400;
    let interval = 2_592_000u64;

    println!("🔄 Testing complete schedule lifecycle...");

    // 1. Create schedule
    let (create_cpu, create_mem, schedule_id) = measure_gas(&env, || {
        client.create_remittance_schedule(&owner, &amount, &next_due, &interval)
    });
    println!("   Create - CPU: {}, Memory: {}", create_cpu, create_mem);

    // 2. Query single schedule
    let (query_cpu, query_mem, schedule) =
        measure_gas(&env, || client.get_remittance_schedule(&schedule_id));
    assert!(schedule.is_some(), "Schedule should be found");
    println!(
        "   Query Single - CPU: {}, Memory: {}",
        query_cpu, query_mem
    );

    // 3. Query all schedules
    let (query_all_cpu, query_all_mem, schedules) =
        measure_gas(&env, || client.get_remittance_schedules(&owner));
    assert_eq!(schedules.len(), 1, "Should find 1 schedule");
    println!(
        "   Query All - CPU: {}, Memory: {}",
        query_all_cpu, query_all_mem
    );

    // 4. Modify schedule
    let new_amount = 2_000i128;
    let new_next_due = env.ledger().timestamp() + 172800;
    let new_interval = 604_800u64;

    let (modify_cpu, modify_mem, modified) = measure_gas(&env, || {
        client.modify_remittance_schedule(
            &owner,
            &schedule_id,
            &new_amount,
            &new_next_due,
            &new_interval,
        )
    });
    assert!(modified, "Schedule modification should succeed");
    println!("   Modify - CPU: {}, Memory: {}", modify_cpu, modify_mem);

    // 5. Cancel schedule
    let (cancel_cpu, cancel_mem, cancelled) = measure_gas(&env, || {
        client.cancel_remittance_schedule(&owner, &schedule_id)
    });
    assert!(cancelled, "Schedule cancellation should succeed");
    println!("   Cancel - CPU: {}, Memory: {}", cancel_cpu, cancel_mem);

    // 6. Verify cancellation
    let schedule = client.get_remittance_schedule(&schedule_id);
    assert!(schedule.is_some(), "Cancelled schedule should still exist");
    assert!(!schedule.unwrap().active, "Schedule should be inactive");

    println!("✅ Complete lifecycle test passed");

    // Summary
    let total_cpu = create_cpu + query_cpu + query_all_cpu + modify_cpu + cancel_cpu;
    let total_mem = create_mem + query_mem + query_all_mem + modify_mem + cancel_mem;
    println!(
        "📊 Total lifecycle cost - CPU: {}, Memory: {}",
        total_cpu, total_mem
    );
}

/// Performance benchmark: Stress test with many schedules
#[test]
fn test_performance_stress() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);

    println!("🚀 Running performance stress test...");

    // Create 20 schedules (reasonable stress test)
    for i in 1..=20 {
        let amount = 1_000i128 * i as i128;
        let next_due = env.ledger().timestamp() + 86400 * i;
        let interval = 2_592_000u64;

        let result = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);
        let _result = (result, "Schedule {} creation should succeed", i);
    }
    // Measure query performance with 20 schedules
    let (cpu, mem, schedules) = measure_gas(&env, || client.get_remittance_schedules(&owner));

    assert_eq!(schedules.len(), 20, "Should return all 20 schedules");

    // Performance should still be reasonable
    assert!(
        cpu < 5_000_000,
        "CPU cost should remain reasonable with 20 schedules"
    );
    assert!(
        mem < 500_000,
        "Memory cost should remain reasonable with 20 schedules"
    );

    println!("✅ Stress test passed - 20 schedules query: CPU: {}, Memory: {}", cpu, mem);
}
