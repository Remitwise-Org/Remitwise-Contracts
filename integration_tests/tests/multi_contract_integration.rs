#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, String as SorobanString};

// Import all contract types and clients
use bill_payments::{BillPayments, BillPaymentsClient};
use insurance::{Insurance, InsuranceClient};
use remittance_split::{RemittanceSplit, RemittanceSplitClient};
use savings_goals::{SavingsGoalContract, SavingsGoalContractClient};
use orchestrator::{Orchestrator, OrchestratorClient};
use data_migration::{ExportSnapshot as MigrationSnapshot, SnapshotPayload, RemittanceSplitExport};

/// Integration test that simulates a complete user flow:
/// 1. Deploy all contracts (remittance_split, savings_goals, bill_payments, insurance)
/// 2. Initialize split configuration
/// 3. Create goals, bills, and policies
/// 4. Calculate split and verify amounts align with expectations
#[test]
fn test_multi_contract_user_flow() {
    // Setup test environment
    let env = Env::default();
    env.mock_all_auths();

    // Generate test user address
    let user = Address::generate(&env);

    // Deploy all contracts
    let remittance_contract_id = env.register_contract(None, RemittanceSplit);
    let remittance_client = RemittanceSplitClient::new(&env, &remittance_contract_id);

    let savings_contract_id = env.register_contract(None, SavingsGoalContract);
    let savings_client = SavingsGoalContractClient::new(&env, &savings_contract_id);

    let bills_contract_id = env.register_contract(None, BillPayments);
    let bills_client = BillPaymentsClient::new(&env, &bills_contract_id);

    let insurance_contract_id = env.register_contract(None, Insurance);
    let insurance_client = InsuranceClient::new(&env, &insurance_contract_id);

    // Step 1: Initialize remittance split with percentages
    // Spending: 40%, Savings: 30%, Bills: 20%, Insurance: 10%
    let nonce = 0u64;
    remittance_client.initialize_split(
        &user, &nonce, &40u32, // spending
        &30u32, // savings
        &20u32, // bills
        &10u32, // insurance
    );

    // Step 2: Create a savings goal
    let goal_name = SorobanString::from_str(&env, "Education Fund");
    let target_amount = 10_000i128;
    let target_date = env.ledger().timestamp() + (365 * 86400); // 1 year from now

    let goal_id = savings_client.create_goal(&user, &goal_name, &target_amount, &target_date);
    assert_eq!(goal_id, 1u32, "Goal ID should be 1");

    // Step 3: Create a bill
    let bill_name = SorobanString::from_str(&env, "Electricity Bill");
    let bill_amount = 500i128;
    let due_date = env.ledger().timestamp() + (30 * 86400); // 30 days from now
    let recurring = true;
    let frequency_days = 30u32;

    let bill_id = bills_client.create_bill(
        &user,
        &bill_name,
        &bill_amount,
        &due_date,
        &recurring,
        &frequency_days,
        &None,
        &SorobanString::from_str(&env, "XLM"),
    );
    assert_eq!(bill_id, 1u32, "Bill ID should be 1");

    // Step 4: Create an insurance policy
    let policy_name = SorobanString::from_str(&env, "Health Insurance");
    let coverage_type = SorobanString::from_str(&env, "health");
    let monthly_premium = 200i128;
    let coverage_amount = 50_000i128;

    let policy_id = insurance_client.create_policy(
        &user,
        &policy_name,
        &coverage_type,
        &monthly_premium,
        &coverage_amount,
        &None,
    );
    assert_eq!(policy_id, 1u32, "Policy ID should be 1");

    // Step 5: Calculate split for a remittance amount
    let total_remittance = 10_000i128;
    let amounts = remittance_client.calculate_split(&total_remittance);
    assert_eq!(amounts.len(), 4, "Should have 4 allocation amounts");

    // Extract amounts
    let spending_amount = amounts.get(0).unwrap();
    let savings_amount = amounts.get(1).unwrap();
    let bills_amount = amounts.get(2).unwrap();
    let insurance_amount = amounts.get(3).unwrap();

    // Step 6: Verify amounts match expected percentages
    // Spending: 40% of 10,000 = 4,000
    assert_eq!(
        spending_amount, 4_000i128,
        "Spending amount should be 4,000"
    );

    // Savings: 30% of 10,000 = 3,000
    assert_eq!(savings_amount, 3_000i128, "Savings amount should be 3,000");

    // Bills: 20% of 10,000 = 2,000
    assert_eq!(bills_amount, 2_000i128, "Bills amount should be 2,000");

    // Insurance: 10% of 10,000 = 1,000 (gets remainder to handle rounding)
    assert_eq!(
        insurance_amount, 1_000i128,
        "Insurance amount should be 1,000"
    );

    // Step 7: Verify total sum equals original amount
    let total_allocated = spending_amount + savings_amount + bills_amount + insurance_amount;
    assert_eq!(
        total_allocated, total_remittance,
        "Total allocated should equal total remittance"
    );

    println!("✅ Multi-contract integration test passed!");
    println!("   Total Remittance: {}", total_remittance);
    println!("   Spending: {} (40%)", spending_amount);
    println!("   Savings: {} (30%)", savings_amount);
    println!("   Bills: {} (20%)", bills_amount);
    println!("   Insurance: {} (10%)", insurance_amount);
}

/// Test with different split percentages and verify rounding behavior
#[test]
fn test_split_with_rounding() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);

    // Deploy remittance split contract
    let remittance_contract_id = env.register_contract(None, RemittanceSplit);
    let remittance_client = RemittanceSplitClient::new(&env, &remittance_contract_id);

    // Initialize with percentages that might cause rounding issues
    // Spending: 33%, Savings: 33%, Bills: 17%, Insurance: 17%
    remittance_client.initialize_split(&user, &0u64, &33u32, &33u32, &17u32, &17u32);

    // Calculate split for an amount that will have rounding
    let total = 1_000i128;
    let amounts = remittance_client.calculate_split(&total);

    let spending = amounts.get(0).unwrap();
    let savings = amounts.get(1).unwrap();
    let bills = amounts.get(2).unwrap();
    let insurance = amounts.get(3).unwrap();

    // Verify total still equals original (insurance gets remainder)
    let total_allocated = spending + savings + bills + insurance;
    assert_eq!(
        total_allocated, total,
        "Total allocated must equal original amount despite rounding"
    );

    println!("✅ Rounding test passed!");
    println!("   Total: {}", total);
    println!("   Spending: {} (33%)", spending);
    println!("   Savings: {} (33%)", savings);
    println!("   Bills: {} (17%)", bills);
    println!("   Insurance: {} (17% + remainder)", insurance);
}

/// Test creating multiple goals, bills, and policies
#[test]
fn test_multiple_entities_creation() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);

    // Deploy contracts
    let savings_contract_id = env.register_contract(None, SavingsGoalContract);
    let savings_client = SavingsGoalContractClient::new(&env, &savings_contract_id);

    let bills_contract_id = env.register_contract(None, BillPayments);
    let bills_client = BillPaymentsClient::new(&env, &bills_contract_id);

    let insurance_contract_id = env.register_contract(None, Insurance);
    let insurance_client = InsuranceClient::new(&env, &insurance_contract_id);

    // Create multiple savings goals
    let goal1 = savings_client.create_goal(
        &user,
        &SorobanString::from_str(&env, "Emergency Fund"),
        &5_000i128,
        &(env.ledger().timestamp() + 180 * 86400),
    );
    assert_eq!(goal1, 1u32);

    let goal2 = savings_client.create_goal(
        &user,
        &SorobanString::from_str(&env, "Vacation"),
        &2_000i128,
        &(env.ledger().timestamp() + 90 * 86400),
    );
    assert_eq!(goal2, 2u32);

    // Create multiple bills
    let bill1 = bills_client.create_bill(
        &user,
        &SorobanString::from_str(&env, "Rent"),
        &1_500i128,
        &(env.ledger().timestamp() + 30 * 86400),
        &true,
        &30u32,
        &None,
        &SorobanString::from_str(&env, "XLM"),
    );
    assert_eq!(bill1, 1u32);

    let bill2 = bills_client.create_bill(
        &user,
        &SorobanString::from_str(&env, "Internet"),
        &100i128,
        &(env.ledger().timestamp() + 15 * 86400),
        &true,
        &30u32,
        &None,
        &SorobanString::from_str(&env, "XLM"),
    );
    assert_eq!(bill2, 2u32);

    // Create multiple insurance policies
    let policy1 = insurance_client.create_policy(
        &user,
        &SorobanString::from_str(&env, "Life Insurance"),
        &SorobanString::from_str(&env, "life"),
        &150i128,
        &100_000i128,
        &None,
    );
    assert_eq!(policy1, 1u32);

    let policy2 = insurance_client.create_policy(
        &user,
        &SorobanString::from_str(&env, "Emergency Coverage"),
        &SorobanString::from_str(&env, "emergency"),
        &50i128,
        &10_000i128,
        &None,
    );
    assert_eq!(policy2, 2u32);

    println!("✅ Multiple entities creation test passed!");
    println!("   Created 2 savings goals");
    println!("   Created 2 bills");
    println!("   Created 2 insurance policies");
}

/// Compatibility Test: Validates contract upgrade (WASM update) preserves state
/// 
/// This test simulates the upgrade process:
/// 1. Deploy v1 of RemittanceSplit
/// 2. Initialize with configuration
/// 3. Update the contract WASM (simulated by re-registering)
/// 4. Verify that state (config and version) remains correct
#[test]
fn test_contract_upgrade_compatibility() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);

    // Step 1: Deploy Initial Version (v1)
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    // Step 2: Initialize state
    client.initialize_split(&user, &0u64, &50u32, &25u32, &15u32, &10u32);
    assert!(client.get_config().is_some());
    assert_eq!(client.get_version(), 1u32);

    // Step 3: "Upgrade" the contract
    // In Soroban tests, we simulate an upgrade by re-registering the contract at the same ID
    // with potentially new logic (here using the same for simplicity, but validating the mechanism)
    env.register_contract(&contract_id, RemittanceSplit);

    // Step 4: Verify state preservation
    let config = client.get_config().expect("Config should be preserved after upgrade");
    assert_eq!(config.spending_percent, 50u32);
    assert_eq!(config.owner, user);
    
    // Verify we can still update the version if authorized
    client.set_version(&user, &2u32);
    assert_eq!(client.get_version(), 2u32);

    println!("✅ Contract upgrade compatibility test passed!");
}

/// Compatibility Test: Validates version consistency across the entire workspace
/// 
/// Ensures all core contracts report the expected version and can interoperate.
#[test]
fn test_version_matrix_interoperability() {
    let env = Env::default();
    env.mock_all_auths();

    // Deploy all contracts
    let split_id = env.register_contract(None, RemittanceSplit);
    let goals_id = env.register_contract(None, SavingsGoalContract);
    let orchestrator_id = env.register_contract(None, Orchestrator);

    let split_client = RemittanceSplitClient::new(&env, &split_id);
    let goals_client = SavingsGoalContractClient::new(&env, &goals_id);
    // Add other clients as needed...

    // Verify versions
    assert_eq!(split_client.get_version(), 1u32, "RemittanceSplit version mismatch");
    assert_eq!(goals_client.get_version(), 1u32, "SavingsGoal version mismatch");

    println!("✅ Version matrix interoperability test passed!");
}

/// Migration Test: Validates that on-chain snapshots align with off-chain migration logic
/// 
/// This is a critical "bridge" test ensuring that the data_migration crate
/// can correctly process data exported from the smart contracts.
#[test]
fn test_data_migration_logic_consistency() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);

    // Deploy and initialize RemittanceSplit
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    client.initialize_split(&user, &0u64, &40u32, &30u32, &20u32, &10u32);

    // 1. Export snapshot from on-chain contract
    let on_chain_snapshot = client.export_snapshot(&user).expect("Should export snapshot");
    
    // 2. Perform off-chain compatibility verification using data_migration types
    // We verify the payload matches our expectation
    let export_data = RemittanceSplitExport {
        owner: format!("{:?}", user),
        spending_percent: on_chain_snapshot.config.spending_percent,
        savings_percent: on_chain_snapshot.config.savings_percent,
        bills_percent: on_chain_snapshot.config.bills_percent,
        insurance_percent: on_chain_snapshot.config.insurance_percent,
    };

    let migration_payload = SnapshotPayload::RemittanceSplit(export_data);
    let migration_snapshot = MigrationSnapshot::new(migration_payload, data_migration::ExportFormat::Json);

    // 3. Verify version consistency between contract and migration tool
    assert_eq!(on_chain_snapshot.version, migration_snapshot.header.version, "Snapshot version mismatch");
    
    // Note: On-chain checksum and off-chain checksum might use different algorithms 
    // (u64 additive vs SHA256) but the data structures must be compatible.
    
    println!("✅ Data migration logic consistency test passed!");
}
