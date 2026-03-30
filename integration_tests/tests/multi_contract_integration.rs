#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, String as SorobanString, IntoVal, Symbol, Val};
use soroban_sdk::testutils::Events;

// Import all contract types and clients
use bill_payments::{BillPayments, BillPaymentsClient};
use insurance::{Insurance, InsuranceClient};
use orchestrator::{Orchestrator, OrchestratorClient, OrchestratorError};
use remittance_split::{RemittanceSplit, RemittanceSplitClient};
use savings_goals::{SavingsGoalContract, SavingsGoalContractClient};

// ============================================================================
// Mock Contracts for Orchestrator Integration Tests
// ============================================================================

use soroban_sdk::{contract, contractimpl, Vec as SorobanVec, vec as soroban_vec};

/// Mock Family Wallet — approves any amount <= 100_000
#[contract]
pub struct MockFamilyWallet;

#[contractimpl]
impl MockFamilyWallet {
    pub fn check_spending_limit(_env: Env, _caller: Address, amount: i128) -> bool {
        amount <= 100_000
    }
}

/// Mock Remittance Split — returns [40%, 30%, 20%, 10%] split
#[contract]
pub struct MockRemittanceSplit;

#[contractimpl]
impl MockRemittanceSplit {
    pub fn calculate_split(env: Env, total_amount: i128) -> SorobanVec<i128> {
        let spending = (total_amount * 40) / 100;
        let savings = (total_amount * 30) / 100;
        let bills = (total_amount * 20) / 100;
        let insurance = total_amount - spending - savings - bills; // remainder
        Vec::from_array(&env, [spending, savings, bills, insurance])
    }
}

/// Mock Savings Goals — panics on goal_id 999 (not found) or 998 (completed)
#[contract]
pub struct MockSavingsGoals;

#[contractimpl]
impl MockSavingsGoals {
    pub fn add_to_goal(_env: Env, _caller: Address, goal_id: u32, amount: i128) -> i128 {
        if goal_id == 999 {
            panic!("Goal not found");
        }
        if goal_id == 998 {
            panic!("Goal already completed");
        }
        amount
    }
}

/// Mock Bill Payments — panics on bill_id 999 (not found) or 998 (already paid)
#[contract]
pub struct MockBillPayments;

#[contractimpl]
impl MockBillPayments {
    pub fn pay_bill(_env: Env, _caller: Address, bill_id: u32) {
        if bill_id == 999 {
            panic!("Bill not found");
        }
        if bill_id == 998 {
            panic!("Bill already paid");
        }
    }
}

/// Mock Insurance — panics on policy_id 999 (not found); returns false for 998 (inactive)
#[contract]
pub struct MockInsurance;

#[contractimpl]
impl MockInsurance {
    pub fn pay_premium(_env: Env, _caller: Address, policy_id: u32) -> bool {
        if policy_id == 999 {
            panic!("Policy not found");
        }
        policy_id != 998
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Deploy all real contracts plus the orchestrator and mock dependency contracts.
/// Returns a tuple of all contract addresses and the test user.
fn setup_full_env() -> (
    Env,
    Address, // remittance_split
    Address, // savings
    Address, // bills
    Address, // insurance
    Address, // orchestrator
    Address, // mock_family_wallet
    Address, // mock_remittance_split
    Address, // user
) {
    let env = Env::default();
    env.mock_all_auths();

    let remittance_id = env.register_contract(None, RemittanceSplit);
    let savings_id = env.register_contract(None, SavingsGoalContract);
    let bills_id = env.register_contract(None, BillPayments);
    let insurance_id = env.register_contract(None, Insurance);
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let mock_family_wallet_id = env.register_contract(None, MockFamilyWallet);
    let mock_split_id = env.register_contract(None, MockRemittanceSplit);

    let user = Address::generate(&env);

    (
        env,
        remittance_id,
        savings_id,
        bills_id,
        insurance_id,
        orchestrator_id,
        mock_family_wallet_id,
        mock_split_id,
        user,
    )
}

// ============================================================================
// Existing Integration Tests (preserved)
// ============================================================================

/// Integration test that simulates a complete user flow:
/// 1. Deploy all contracts (remittance_split, savings_goals, bill_payments, insurance)
/// 2. Initialize split configuration
/// 3. Create goals, bills, and policies
/// 4. Calculate split and verify amounts align with expectations
#[test]
fn test_multi_contract_user_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);

    let remittance_contract_id = env.register_contract(None, RemittanceSplit);
    let remittance_client = RemittanceSplitClient::new(&env, &remittance_contract_id);

    let savings_contract_id = env.register_contract(None, SavingsGoalContract);
    let savings_client = SavingsGoalContractClient::new(&env, &savings_contract_id);

    let bills_contract_id = env.register_contract(None, BillPayments);
    let bills_client = BillPaymentsClient::new(&env, &bills_contract_id);

    let insurance_contract_id = env.register_contract(None, Insurance);
    let insurance_client = InsuranceClient::new(&env, &insurance_contract_id);

    let nonce = 0u64;
    let mock_usdc = Address::generate(&env);
    remittance_client.initialize_split(&user, &nonce, &mock_usdc, &40u32, &30u32, &20u32, &10u32);

    let goal_name = SorobanString::from_str(&env, "Education Fund");
    let target_amount = 10_000i128;
    let target_date = env.ledger().timestamp() + (365 * 86400);

    let goal_id = savings_client.create_goal(&user, &goal_name, &target_amount, &target_date);
    assert_eq!(goal_id, 1u32, "Goal ID should be 1");

    let bill_name = SorobanString::from_str(&env, "Electricity Bill");
    let bill_amount = 500i128;
    let due_date = env.ledger().timestamp() + (30 * 86400);

    let bill_id = bills_client.create_bill(
        &user,
        &bill_name,
        &bill_amount,
        &due_date,
        &true,
        &30u32,
        &SorobanString::from_str(&env, "XLM"),
    );
    assert_eq!(bill_id, 1u32, "Bill ID should be 1");

    let policy_id = insurance_client.create_policy(
        &user,
        &SorobanString::from_str(&env, "Health Insurance"),
        &CoverageType::Health,
        &200i128,
        &50_000i128,
    );
    assert_eq!(policy_id, 1u32, "Policy ID should be 1");

    let total_remittance = 10_000i128;
    let amounts = remittance_client.calculate_split(&total_remittance);
    assert_eq!(amounts.len(), 4, "Should have 4 allocation amounts");

    let spending_amount = amounts.get(0).unwrap();
    let savings_amount = amounts.get(1).unwrap();
    let bills_amount = amounts.get(2).unwrap();
    let insurance_amount = amounts.get(3).unwrap();

    assert_eq!(
        spending_amount, 4_000i128,
        "Spending amount should be 4,000"
    );
    assert_eq!(savings_amount, 3_000i128, "Savings amount should be 3,000");
    assert_eq!(bills_amount, 2_000i128, "Bills amount should be 2,000");
    assert_eq!(
        insurance_amount, 1_000i128,
        "Insurance amount should be 1,000"
    );

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

#[test]
fn test_split_with_rounding() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);
    let mock_usdc = Address::generate(&env);

    let remittance_contract_id = env.register_contract(None, RemittanceSplit);
    let remittance_client = RemittanceSplitClient::new(&env, &remittance_contract_id);

    remittance_client.initialize_split(&user, &0u64, &mock_usdc, &33u32, &33u32, &17u32, &17u32);

    let total = 1_000i128;
    let amounts = remittance_client.calculate_split(&total);

    let spending = amounts.get(0).unwrap();
    let savings = amounts.get(1).unwrap();
    let bills = amounts.get(2).unwrap();
    let insurance = amounts.get(3).unwrap();

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

#[test]
fn test_multiple_entities_creation() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);

    let savings_contract_id = env.register_contract(None, SavingsGoalContract);
    let savings_client = SavingsGoalContractClient::new(&env, &savings_contract_id);

    let bills_contract_id = env.register_contract(None, BillPayments);
    let bills_client = BillPaymentsClient::new(&env, &bills_contract_id);

    let insurance_contract_id = env.register_contract(None, Insurance);
    let insurance_client = InsuranceClient::new(&env, &insurance_contract_id);

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

    let bill1 = bills_client.create_bill(
        &user,
        &SorobanString::from_str(&env, "Rent"),
        &1_500i128,
        &(env.ledger().timestamp() + 30 * 86400),
        &true,
        &30u32,
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
        &SorobanString::from_str(&env, "XLM"),
    );
    assert_eq!(bill2, 2u32);

    let policy1 = insurance_client.create_policy(
        &user,
        &SorobanString::from_str(&env, "Life Insurance"),
        &SorobanString::from_str(&env, "life"),
        &150i128,
        &100_000i128,
    );
    assert_eq!(policy1, 1u32);

    let policy2 = insurance_client.create_policy(
        &user,
        &SorobanString::from_str(&env, "Emergency Coverage"),
        &SorobanString::from_str(&env, "emergency"),
        &50i128,
        &10_000i128,
    );
    assert_eq!(policy2, 2u32);

    println!("✅ Multiple entities creation test passed!");
}

// ============================================================================
// Rollback Integration Tests — Savings Leg Failures
// ============================================================================

/// INT-ROLLBACK-01: Full orchestrator flow rolls back when savings leg fails (goal not found).
/// Verifies that the Soroban transaction reverts atomically when a cross-contract
/// savings call panics, leaving no partial state in any downstream contract.
#[test]
fn test_integration_rollback_savings_leg_goal_not_found() {
    let (
        env,
        _,
        mock_savings_id,
        mock_bills_id,
        mock_insurance_id,
        orchestrator_id,
        mock_family_wallet_id,
        mock_split_id,
        user,
    ) = {
        let env = Env::default();
        env.mock_all_auths();
        let orchestrator_id = env.register_contract(None, Orchestrator);
        let mock_family_wallet_id = env.register_contract(None, MockFamilyWallet);
        let mock_split_id = env.register_contract(None, MockRemittanceSplit);
        let mock_savings_id = env.register_contract(None, MockSavingsGoals);
        let mock_bills_id = env.register_contract(None, MockBillPayments);
        let mock_insurance_id = env.register_contract(None, MockInsurance);
        let user = Address::generate(&env);
        (
            env,
            mock_split_id.clone(),
            mock_savings_id,
            mock_bills_id,
            mock_insurance_id,
            orchestrator_id,
            mock_family_wallet_id,
            mock_split_id,
            user,
        )
    };

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    // Savings fails at goal_id=999 — should trigger full rollback
    let result = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &mock_family_wallet_id,
        &mock_split_id,
        &mock_savings_id,
        &mock_bills_id,
        &mock_insurance_id,
        &999, // savings fails here
        &1,
        &1,
    );

    assert!(
        result.is_err(),
        "INT-ROLLBACK-01: Flow must roll back when savings leg panics"
    );

    println!("✅ INT-ROLLBACK-01 passed: savings failure triggers full rollback");
}

/// INT-ROLLBACK-02: Full orchestrator flow rolls back when bills leg fails
/// after savings has already been processed in the same transaction.
#[test]
fn test_integration_rollback_bills_leg_after_savings_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let mock_family_wallet_id = env.register_contract(None, MockFamilyWallet);
    let mock_split_id = env.register_contract(None, MockRemittanceSplit);
    let mock_savings_id = env.register_contract(None, MockSavingsGoals);
    let mock_bills_id = env.register_contract(None, MockBillPayments);
    let mock_insurance_id = env.register_contract(None, MockInsurance);
    let user = Address::generate(&env);

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    // Savings succeeds (goal_id=1), bills fails (bill_id=999)
    // Soroban atomicity guarantees savings is also rolled back
    let result = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &mock_family_wallet_id,
        &mock_split_id,
        &mock_savings_id,
        &mock_bills_id,
        &mock_insurance_id,
        &1,
        &999, // bills fails after savings completes
        &1,
    );

    assert!(
        result.is_err(),
        "INT-ROLLBACK-02: Flow must roll back savings + bills when bills leg panics"
    );

    println!("✅ INT-ROLLBACK-02 passed: bills failure after savings triggers full rollback");
}

/// INT-ROLLBACK-03: Full orchestrator flow rolls back when insurance leg fails
/// after both savings and bills have been processed in the same transaction.
#[test]
fn test_integration_rollback_insurance_leg_after_savings_and_bills_succeed() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let mock_family_wallet_id = env.register_contract(None, MockFamilyWallet);
    let mock_split_id = env.register_contract(None, MockRemittanceSplit);
    let mock_savings_id = env.register_contract(None, MockSavingsGoals);
    let mock_bills_id = env.register_contract(None, MockBillPayments);
    let mock_insurance_id = env.register_contract(None, MockInsurance);
    let user = Address::generate(&env);

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    // Savings succeeds (goal_id=1), bills succeeds (bill_id=1),
    // insurance fails (policy_id=999) — all prior changes must revert
    let result = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &mock_family_wallet_id,
        &mock_split_id,
        &mock_savings_id,
        &mock_bills_id,
        &mock_insurance_id,
        &1,
        &1,
        &999, // insurance fails last
    );

    assert!(
        result.is_err(),
        "INT-ROLLBACK-03: Flow must roll back all legs when insurance leg panics"
    );

    println!(
        "✅ INT-ROLLBACK-03 passed: insurance failure after savings+bills triggers full rollback"
    );
}

// ============================================================================
// Rollback Integration Tests — Already-Paid / Duplicate Protection
// ============================================================================

/// INT-ROLLBACK-04: Duplicate bill payment attempt rolls back the entire flow.
/// Verifies that double-payment protection in the bills contract causes
/// a full transaction rollback.
#[test]
fn test_integration_rollback_duplicate_bill_payment() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let mock_family_wallet_id = env.register_contract(None, MockFamilyWallet);
    let mock_split_id = env.register_contract(None, MockRemittanceSplit);
    let mock_savings_id = env.register_contract(None, MockSavingsGoals);
    let mock_bills_id = env.register_contract(None, MockBillPayments);
    let mock_insurance_id = env.register_contract(None, MockInsurance);
    let user = Address::generate(&env);

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    // bill_id=998 simulates an already-paid bill
    let result = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &mock_family_wallet_id,
        &mock_split_id,
        &mock_savings_id,
        &mock_bills_id,
        &mock_insurance_id,
        &1,
        &998, // already paid
        &1,
    );

    assert!(
        result.is_err(),
        "INT-ROLLBACK-04: Duplicate bill payment must trigger full rollback"
    );

    println!("✅ INT-ROLLBACK-04 passed: duplicate bill triggers rollback");
}

/// INT-ROLLBACK-05: Completed savings goal rejects deposit and triggers rollback.
#[test]
fn test_integration_rollback_completed_savings_goal() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let mock_family_wallet_id = env.register_contract(None, MockFamilyWallet);
    let mock_split_id = env.register_contract(None, MockRemittanceSplit);
    let mock_savings_id = env.register_contract(None, MockSavingsGoals);
    let mock_bills_id = env.register_contract(None, MockBillPayments);
    let mock_insurance_id = env.register_contract(None, MockInsurance);
    let user = Address::generate(&env);

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    // goal_id=998 simulates a fully funded/completed goal
    let result = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &mock_family_wallet_id,
        &mock_split_id,
        &mock_savings_id,
        &mock_bills_id,
        &mock_insurance_id,
        &998, // completed goal
        &1,
        &1,
    );

    assert!(
        result.is_err(),
        "INT-ROLLBACK-05: Completed savings goal must trigger full rollback"
    );

    println!("✅ INT-ROLLBACK-05 passed: completed goal triggers rollback");
}

// ============================================================================
// Rollback Integration Tests — Accounting Consistency
// ============================================================================

/// INT-ACCOUNTING-01: Verify remittance split allocations sum to total across contracts.
/// Deploys real remittance split and verifies no funds leak during allocation.
#[test]
fn test_integration_accounting_split_sums_to_total() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);
    let remittance_id = env.register_contract(None, RemittanceSplit);
    let remittance_client = RemittanceSplitClient::new(&env, &remittance_id);

    let mock_usdc = Address::generate(&env);
    remittance_client.initialize_split(&user, &0u64, &mock_usdc, &40u32, &30u32, &20u32, &10u32);

    for total in [1_000i128, 9_999i128, 10_000i128, 77_777i128] {
        let amounts = remittance_client.calculate_split(&total);
        let sum: i128 = (0..amounts.len())
            .map(|i| amounts.get(i).unwrap_or(0))
            .sum();
        assert_eq!(
            sum, total,
            "INT-ACCOUNTING-01: Split must sum to {} (got {})",
            total, sum
        );
    }

    println!("✅ INT-ACCOUNTING-01 passed: split sums verified across multiple amounts");
}

/// INT-ACCOUNTING-02: Successful orchestrator flow returns consistent allocation metadata.
/// Verifies the RemittanceFlowResult fields reflect the actual split percentages.
#[test]
fn test_integration_accounting_flow_result_consistency() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let mock_family_wallet_id = env.register_contract(None, MockFamilyWallet);
    let mock_split_id = env.register_contract(None, MockRemittanceSplit);
    let mock_savings_id = env.register_contract(None, MockSavingsGoals);
    let mock_bills_id = env.register_contract(None, MockBillPayments);
    let mock_insurance_id = env.register_contract(None, MockInsurance);
    let user = Address::generate(&env);

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    let total = 10_000i128;
    let result = client.try_execute_remittance_flow(
        &user,
        &total,
        &mock_family_wallet_id,
        &mock_split_id,
        &mock_savings_id,
        &mock_bills_id,
        &mock_insurance_id,
        &1,
        &1,
        &1,
    );

    assert!(result.is_ok());
    let flow = result.unwrap().unwrap();

    // Verify total preserved
    assert_eq!(flow.total_amount, total);

    // Verify split percentages (mock: 40/30/20/10)
    assert_eq!(flow.spending_amount, 4_000, "Spending must be 40%");
    assert_eq!(flow.savings_amount, 3_000, "Savings must be 30%");
    assert_eq!(flow.bills_amount, 2_000, "Bills must be 20%");
    assert_eq!(flow.insurance_amount, 1_000, "Insurance must be 10%");

    // Verify allocations sum to total
    let allocated =
        flow.spending_amount + flow.savings_amount + flow.bills_amount + flow.insurance_amount;
    assert_eq!(
        allocated, total,
        "INT-ACCOUNTING-02: Allocations must sum to total"
    );

    // Verify all legs succeeded
    assert!(flow.savings_success);
    assert!(flow.bills_success);
    assert!(flow.insurance_success);

    println!("✅ INT-ACCOUNTING-02 passed: flow result accounting is consistent");
}

// ============================================================================
// Rollback Integration Tests — Recovery After Failure
// ============================================================================

/// INT-RECOVERY-01: A failed flow does not block a subsequent successful flow.
/// Verifies that Soroban's rollback leaves contracts in their original state,
/// ready to accept the next valid transaction.
#[test]
fn test_integration_recovery_after_savings_failure() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let mock_family_wallet_id = env.register_contract(None, MockFamilyWallet);
    let mock_split_id = env.register_contract(None, MockRemittanceSplit);
    let mock_savings_id = env.register_contract(None, MockSavingsGoals);
    let mock_bills_id = env.register_contract(None, MockBillPayments);
    let mock_insurance_id = env.register_contract(None, MockInsurance);
    let user = Address::generate(&env);

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    // First transaction: savings fails
    let fail = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &mock_family_wallet_id,
        &mock_split_id,
        &mock_savings_id,
        &mock_bills_id,
        &mock_insurance_id,
        &999,
        &1,
        &1,
    );
    assert!(fail.is_err(), "First flow must fail");

    // Second transaction: all valid — must succeed without any residual state from failure
    let success = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &mock_family_wallet_id,
        &mock_split_id,
        &mock_savings_id,
        &mock_bills_id,
        &mock_insurance_id,
        &1,
        &1,
        &1,
    );
    assert!(
        success.is_ok(),
        "INT-RECOVERY-01: Subsequent valid flow must succeed after a rolled-back failure"
    );

    println!("✅ INT-RECOVERY-01 passed: contract state recovered cleanly after rollback");
}

/// INT-RECOVERY-02: A failed bills flow does not block a subsequent successful flow.
#[test]
fn test_integration_recovery_after_bills_failure() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let mock_family_wallet_id = env.register_contract(None, MockFamilyWallet);
    let mock_split_id = env.register_contract(None, MockRemittanceSplit);
    let mock_savings_id = env.register_contract(None, MockSavingsGoals);
    let mock_bills_id = env.register_contract(None, MockBillPayments);
    let mock_insurance_id = env.register_contract(None, MockInsurance);
    let user = Address::generate(&env);

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    let fail = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &mock_family_wallet_id,
        &mock_split_id,
        &mock_savings_id,
        &mock_bills_id,
        &mock_insurance_id,
        &1,
        &999,
        &1,
    );
    assert!(fail.is_err(), "First flow must fail");

    let success = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &mock_family_wallet_id,
        &mock_split_id,
        &mock_savings_id,
        &mock_bills_id,
        &mock_insurance_id,
        &1,
        &1,
        &1,
    );
    assert!(
        success.is_ok(),
        "INT-RECOVERY-02: Subsequent valid flow must succeed after bills rollback"
    );

    println!("✅ INT-RECOVERY-02 passed: contract state recovered after bills failure rollback");
}

/// INT-RECOVERY-03: A failed insurance flow does not block a subsequent successful flow.
#[test]
fn test_integration_recovery_after_insurance_failure() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let mock_family_wallet_id = env.register_contract(None, MockFamilyWallet);
    let mock_split_id = env.register_contract(None, MockRemittanceSplit);
    let mock_savings_id = env.register_contract(None, MockSavingsGoals);
    let mock_bills_id = env.register_contract(None, MockBillPayments);
    let mock_insurance_id = env.register_contract(None, MockInsurance);
    let user = Address::generate(&env);

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    let fail = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &mock_family_wallet_id,
        &mock_split_id,
        &mock_savings_id,
        &mock_bills_id,
        &mock_insurance_id,
        &1,
        &1,
        &999,
    );
    assert!(fail.is_err(), "First flow must fail");

    let success = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &mock_family_wallet_id,
        &mock_split_id,
        &mock_savings_id,
        &mock_bills_id,
        &mock_insurance_id,
        &1,
        &1,
        &1,
    );
    assert!(
        success.is_ok(),
        "INT-RECOVERY-03: Subsequent valid flow must succeed after insurance rollback"
    );

    println!(
        "✅ INT-RECOVERY-03 passed: contract state recovered after insurance failure rollback"
    );
}

// ============================================================================
// Rollback Integration Tests — Permission Failures
// ============================================================================

/// INT-PERMISSION-01: Permission denied stops the flow before any downstream contract is called.
#[test]
fn test_integration_permission_denied_stops_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let mock_family_wallet_id = env.register_contract(None, MockFamilyWallet);
    let mock_split_id = env.register_contract(None, MockRemittanceSplit);
    let mock_savings_id = env.register_contract(None, MockSavingsGoals);
    let mock_bills_id = env.register_contract(None, MockBillPayments);
    let mock_insurance_id = env.register_contract(None, MockInsurance);
    let user = Address::generate(&env);

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    // 100_001 > 100_000 limit — permission denied
    let result = client.try_execute_remittance_flow(
        &user,
        &100_001,
        &mock_family_wallet_id,
        &mock_split_id,
        &mock_savings_id,
        &mock_bills_id,
        &mock_insurance_id,
        &1,
        &1,
        &1,
    );

    assert!(
        result.is_err(),
        "INT-PERMISSION-01: Flow must be rejected when spending limit is exceeded"
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        OrchestratorError::PermissionDenied,
        "Error must be PermissionDenied"
    );

    println!("✅ INT-PERMISSION-01 passed: permission denial stops flow before downstream calls");
}

/// INT-PERMISSION-02: Zero and negative amounts are rejected before any contract is called.
#[test]
fn test_integration_invalid_amounts_rejected_early() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let mock_family_wallet_id = env.register_contract(None, MockFamilyWallet);
    let mock_split_id = env.register_contract(None, MockRemittanceSplit);
    let mock_savings_id = env.register_contract(None, MockSavingsGoals);
    let mock_bills_id = env.register_contract(None, MockBillPayments);
    let mock_insurance_id = env.register_contract(None, MockInsurance);
    let user = Address::generate(&env);

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    for invalid_amount in [0i128, -1i128, -100_000i128] {
        let result = client.try_execute_remittance_flow(
            &user,
            &invalid_amount,
            &mock_family_wallet_id,
            &mock_split_id,
            &mock_savings_id,
            &mock_bills_id,
            &mock_insurance_id,
            &1,
            &1,
            &1,
        );
        assert!(
            result.is_err(),
            "INT-PERMISSION-02: Amount {} must be rejected",
            invalid_amount
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::InvalidAmount,
            "Amount {} must produce InvalidAmount error",
            invalid_amount
        );
    }

    println!("✅ INT-PERMISSION-02 passed: invalid amounts rejected before downstream calls");
}

/// Workspace-wide event topic compliance tests.
///
/// These tests verify that events emitted by key contracts follow the
/// deterministic Remitwise topic schema:
/// `("Remitwise", category: u32, priority: u32, action: Symbol)`.
///
/// The test triggers representative actions in each contract and inspects
/// `env.events().all()` to validate topics and payload shapes. Any deviation
/// will cause the test to fail, highlighting contracts that must be updated
/// to the shared `RemitwiseEvents` helper.
#[test]
fn test_event_topic_compliance_across_contracts() {
    use soroban_sdk::{symbol_short, IntoVal, Vec};

    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);

    // Deploy representative contracts
    let remittance_id = env.register_contract(None, RemittanceSplit);
    let remittance_client = RemittanceSplitClient::new(&env, &remittance_id);

    let savings_id = env.register_contract(None, SavingsGoalContract);
    let savings_client = SavingsGoalContractClient::new(&env, &savings_id);

    let bills_id = env.register_contract(None, BillPayments);
    let bills_client = BillPaymentsClient::new(&env, &bills_id);

    let insurance_id = env.register_contract(None, Insurance);
    let insurance_client = InsuranceClient::new(&env, &insurance_id);

    // Trigger events in each contract
    let mock_usdc = Address::generate(&env);
    remittance_client.initialize_split(&user, &0u64, &mock_usdc, &40u32, &30u32, &20u32, &10u32);

    let goal_name = SorobanString::from_str(&env, "Compliance Goal");
    let _ = savings_client.create_goal(
        &user,
        &goal_name,
        &1000i128,
        &(env.ledger().timestamp() + 86400),
    );

    let bill_name = SorobanString::from_str(&env, "Compliance Bill");
    let _ = bills_client.create_bill(
        &user,
        &bill_name,
        &100i128,
        &(env.ledger().timestamp() + 86400),
        &true,
        &30u32,
        &None,
        &SorobanString::from_str(&env, "XLM"),
    );

    let policy_name = SorobanString::from_str(&env, "Compliance Policy");
    let _ = insurance_client.create_policy(&user, &policy_name, &CoverageType::Health, &50i128, &1000i128);

    // Collect published events
    let events = env.events().all();
    assert!(
        events.len() > 0,
        "No events were emitted by the sample actions"
    );

    // Validate each event's topics conform to Remitwise schema
    let mut non_compliant = Vec::new(&env);

    for ev in events.iter() {
        let topics = &ev.1;
        // Expect topics to be a vector of length 4 starting with symbol_short!("Remitwise")
        let ok = topics.len() == 4
            && topics.get(0).unwrap() == symbol_short!("Remitwise").into_val(&env);
        if !ok {
            non_compliant.push_back(ev.clone());
            eprintln!("Non-compliant event found: Topics={:?}, Data={:?}", topics, ev.2);
        }
    }

    // Fail if any non-compliant events found, listing one example for debugging
        assert_eq!(non_compliant.len(), 0u32, "Found events that do not follow the Remitwise topic schema. See EVENTS.md and remitwise-common::RemitwiseEvents for guidance.");
}

// ============================================================================
// Stress Integration Tests — Batch Execution & High Volume
// ============================================================================

/// INT-STRESS-01: High-volume batch execution (20 flows).
/// Verifies that the orchestrator can handle a large batch of valid flows
/// in a single transaction without exceeding gas limits.
#[test]
fn test_integration_stress_high_volume_batch_success() {
    let (env, _, mock_savings_id, mock_bills_id, mock_insurance_id,
         orchestrator_id, mock_family_wallet_id, mock_split_id, user) = setup_full_env();

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    let mut flows = SorobanVec::new(&env);
    for _ in 0..20 {
        flows.push_back(RemittanceFlowArgs {
            total_amount: 1000,
            family_wallet_addr: mock_family_wallet_id.clone(),
            remittance_split_addr: mock_split_id.clone(),
            savings_addr: mock_savings_id.clone(),
            bills_addr: mock_bills_id.clone(),
            insurance_addr: mock_insurance_id.clone(),
            goal_id: 1,
            bill_id: 1,
            policy_id: 1,
        });
    }

    let result = client.try_execute_remittance_batch(&user, &flows);

    assert!(result.is_ok(), "STRESS-01: High-volume batch must succeed");
    let batch_results = result.unwrap().unwrap();
    assert_eq!(batch_results.len(), 20);

    for res in batch_results.iter() {
        let _ = res.expect("Flow in batch should be Ok");
    }

    println!("✅ STRESS-01 passed: 20-flow batch processed successfully");
}

/// INT-STRESS-02: Mixed success/failure batch.
/// Verifies that the batch continues processing when individual flows fail
/// (e.g., due to invalid IDs or spending limits).
#[test]
fn test_integration_stress_mixed_batch() {
    let (env, _, mock_savings_id, mock_bills_id, mock_insurance_id,
         orchestrator_id, mock_family_wallet_id, mock_split_id, user) = setup_full_env();

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    let mut flows = SorobanVec::new(&env);

    // 1. Valid flow
    flows.push_back(RemittanceFlowArgs {
        total_amount: 1000,
        family_wallet_addr: mock_family_wallet_id.clone(),
        remittance_split_addr: mock_split_id.clone(),
        savings_addr: mock_savings_id.clone(),
        bills_addr: mock_bills_id.clone(),
        insurance_addr: mock_insurance_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    });

    // 2. Invalid flow (savings goal not found-999)
    flows.push_back(RemittanceFlowArgs {
        total_amount: 1000,
        family_wallet_addr: mock_family_wallet_id.clone(),
        remittance_split_addr: mock_split_id.clone(),
        savings_addr: mock_savings_id.clone(),
        bills_addr: mock_bills_id.clone(),
        insurance_addr: mock_insurance_id.clone(),
        goal_id: 999,
        bill_id: 1,
        policy_id: 1,
    });

    // 3. Invalid flow (spending limit exceeded-200,000 > 100,000)
    flows.push_back(RemittanceFlowArgs {
        total_amount: 200_000,
        family_wallet_addr: mock_family_wallet_id.clone(),
        remittance_split_addr: mock_split_id.clone(),
        savings_addr: mock_savings_id.clone(),
        bills_addr: mock_bills_id.clone(),
        insurance_addr: mock_insurance_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    });

    // 4. Valid flow
    flows.push_back(RemittanceFlowArgs {
        total_amount: 500,
        family_wallet_addr: mock_family_wallet_id.clone(),
        remittance_split_addr: mock_split_id.clone(),
        savings_addr: mock_savings_id.clone(),
        bills_addr: mock_bills_id.clone(),
        insurance_addr: mock_insurance_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    });

    let result = client.try_execute_remittance_batch(&user, &flows);

    assert!(result.is_ok());
    let batch_results = result.unwrap().unwrap();
    assert_eq!(batch_results.len(), 4);

    assert!(batch_results.get(0).unwrap().is_ok(), "Flow 1 should succeed");
    assert!(batch_results.get(1).unwrap().is_err(), "Flow 2 should fail (savings)");
    assert!(batch_results.get(2).unwrap().is_err(), "Flow 3 should fail (limit)");
    assert!(batch_results.get(3).unwrap().is_ok(),  "Flow 4 should succeed");

    // Type hint for Result
    let _: Result<RemittanceFlowResult, OrchestratorError> = batch_results.get(0).unwrap();

    println!("✅ STRESS-02 passed: Mixed success/failure batch tracked correctly");
}

/// INT-STRESS-03: Repeated batch execution.
/// Verifies that repeated batch calls do not cause state corruption
/// or unexpected gas escalations.
#[test]
fn test_integration_stress_repeated_batches() {
    let (env, _, mock_savings_id, mock_bills_id, mock_insurance_id,
         orchestrator_id, mock_family_wallet_id, mock_split_id, user) = setup_full_env();

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    for i in 0..5 {
        let mut flows = SorobanVec::new(&env);
        for _ in 0..10 {
            flows.push_back(RemittanceFlowArgs {
                total_amount: 100,
                family_wallet_addr: mock_family_wallet_id.clone(),
                remittance_split_addr: mock_split_id.clone(),
                savings_addr: mock_savings_id.clone(),
                bills_addr: mock_bills_id.clone(),
                insurance_addr: mock_insurance_id.clone(),
                goal_id: 1,
                bill_id: 1,
                policy_id: 1,
            });
        }
        let result = client.try_execute_remittance_batch(&user, &flows);
        assert!(result.is_ok(), "Batch {} must succeed", i);
    }

    println!("✅ STRESS-03 passed: 5 consecutive batches processed cleanly");
}

// ============================================================================
// Insurance Failure Tests
// ============================================================================

/// @notice Verifies inactive insurance policy fails orchestrated flow safely.
/// @dev Checks that downstream writes in savings and bills are reverted.
#[test]
fn test_orchestrator_flow_inactive_policy_reverts_downstream_state() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let wallet_id = env.register_contract(None, MockFamilyWallet);
    let split_id = env.register_contract(None, MockRemittanceSplit);
    let savings_id = env.register_contract(None, SavingsGoalContract);
    let bills_id = env.register_contract(None, BillPayments);
    let insurance_id = env.register_contract(None, Insurance);

    let orchestrator_client = OrchestratorClient::new(&env, &orchestrator_id);
    let savings_client = SavingsGoalContractClient::new(&env, &savings_id);
    let bills_client = BillPaymentsClient::new(&env, &bills_id);
    let insurance_client = InsuranceClient::new(&env, &insurance_id);

    let goal_id = savings_client.create_goal(
        &user,
        &SorobanString::from_str(&env, "Safety Goal"),
        &10_000i128,
        &(env.ledger().timestamp() + 365 * 86400),
    );
    let bill_id = bills_client.create_bill(
        &user,
        &SorobanString::from_str(&env, "Safety Bill"),
        &500i128,
        &(env.ledger().timestamp() + 30 * 86400),
        &true,
        &30u32,
        &SorobanString::from_str(&env, "XLM"),
    );
    let policy_id = insurance_client.create_policy(
        &user,
        &SorobanString::from_str(&env, "Safety Policy"),
        &SorobanString::from_str(&env, "health"),
        &200i128,
        &25_000i128,
    );
    insurance_client.deactivate_policy(&user, &policy_id);

    let result = orchestrator_client.try_execute_remittance_flow(
        &user,
        &10_000i128,
        &wallet_id,
        &split_id,
        &savings_id,
        &bills_id,
        &insurance_id,
        &goal_id,
        &bill_id,
        &policy_id,
    );
    assert!(result.is_err());

    let goal_after = savings_client.get_goal(&goal_id).unwrap();
    assert_eq!(goal_after.current_amount, 0, "Savings mutation must rollback");

    let bill_after = bills_client.get_bill(&bill_id).unwrap();
    assert!(!bill_after.paid, "Bill payment mutation must rollback");
}

/// @notice Verifies missing insurance policy fails orchestrated flow safely.
/// @dev Uses unknown `policy_id` and asserts no persisted mutations.
#[test]
fn test_orchestrator_flow_missing_policy_reverts_downstream_state() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let wallet_id = env.register_contract(None, MockFamilyWallet);
    let split_id = env.register_contract(None, MockRemittanceSplit);
    let savings_id = env.register_contract(None, SavingsGoalContract);
    let bills_id = env.register_contract(None, BillPayments);
    let insurance_id = env.register_contract(None, Insurance);

    let orchestrator_client = OrchestratorClient::new(&env, &orchestrator_id);
    let savings_client = SavingsGoalContractClient::new(&env, &savings_id);
    let bills_client = BillPaymentsClient::new(&env, &bills_id);

    let goal_id = savings_client.create_goal(
        &user,
        &SorobanString::from_str(&env, "Missing Policy Goal"),
        &10_000i128,
        &(env.ledger().timestamp() + 365 * 86400),
    );
    let bill_id = bills_client.create_bill(
        &user,
        &SorobanString::from_str(&env, "Missing Policy Bill"),
        &500i128,
        &(env.ledger().timestamp() + 30 * 86400),
        &true,
        &30u32,
        &SorobanString::from_str(&env, "XLM"),
    );

    let result = orchestrator_client.try_execute_remittance_flow(
        &user,
        &10_000i128,
        &wallet_id,
        &split_id,
        &savings_id,
        &bills_id,
        &insurance_id,
        &goal_id,
        &bill_id,
        &999_999u32, // missing policy ID
    );
    assert!(result.is_err());

    let goal_after = savings_client.get_goal(&goal_id).unwrap();
    assert_eq!(goal_after.current_amount, 0, "Savings mutation must rollback");

    let bill_after = bills_client.get_bill(&bill_id).unwrap();
    assert!(!bill_after.paid, "Bill payment mutation must rollback");
}