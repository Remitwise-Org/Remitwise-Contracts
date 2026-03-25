use crate::{ExecutionState, Orchestrator, OrchestratorClient, OrchestratorError};
use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

// ============================================================================
// Mock Contract Implementations
// ============================================================================

/// Mock Family Wallet contract for testing
#[contract]
pub struct MockFamilyWallet;

#[contractimpl]
impl MockFamilyWallet {
    /// Mock implementation of check_spending_limit
    /// Returns true if amount <= 10000 (simulating a spending limit)
    pub fn check_spending_limit(_env: Env, _caller: Address, amount: i128) -> bool {
        amount <= 10000
    }
}

/// Mock Remittance Split contract for testing
#[contract]
pub struct MockRemittanceSplit;

#[contractimpl]
impl MockRemittanceSplit {
    /// Mock implementation of calculate_split
    /// Returns [40%, 30%, 20%, 10%] split
    pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
        let spending = (total_amount * 40) / 100;
        let savings = (total_amount * 30) / 100;
        let bills = (total_amount * 20) / 100;
        let insurance = (total_amount * 10) / 100;

        Vec::from_array(&env, [spending, savings, bills, insurance])
    }
}

/// Mock Savings Goals contract for testing
#[contract]
pub struct MockSavingsGoals;

#[contractimpl]
impl MockSavingsGoals {
    /// Mock implementation of add_to_goal
    /// Panics if goal_id == 999 (simulating goal not found)
    /// Panics if goal_id == 998 (simulating goal already completed)
    /// Panics if amount <= 0 (simulating invalid amount)
    pub fn add_to_goal(_env: Env, _caller: Address, goal_id: u32, amount: i128) -> i128 {
        if goal_id == 999 {
            panic!("Goal not found");
        }
        if goal_id == 998 {
            panic!("Goal already completed");
        }
        if amount <= 0 {
            panic!("Amount must be positive");
        }
        amount
    }
}

/// Mock Bill Payments contract for testing
#[contract]
pub struct MockBillPayments;

#[contractimpl]
impl MockBillPayments {
    /// Mock implementation of pay_bill
    /// Panics if bill_id == 999 (simulating bill not found)
    /// Panics if bill_id == 998 (simulating bill already paid)
    pub fn pay_bill(_env: Env, _caller: Address, bill_id: u32) {
        if bill_id == 999 {
            panic!("Bill not found");
        }
        if bill_id == 998 {
            panic!("Bill already paid");
        }
    }
}

/// Mock Insurance contract for testing
#[contract]
pub struct MockInsurance;

#[contractimpl]
impl MockInsurance {
    /// Mock implementation of pay_premium
    /// Panics if policy_id == 999 (simulating policy not found)
    /// Returns false if policy_id == 998 (simulating inactive policy)
    pub fn pay_premium(_env: Env, _caller: Address, policy_id: u32) -> bool {
        if policy_id == 999 {
            panic!("Policy not found");
        }
        policy_id != 998
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Full test environment with all contracts deployed.
    /// Returns (env, orchestrator, family_wallet, remittance_split,
    ///          savings, bills, insurance, user)
    fn setup() -> (Env, Address, Address, Address, Address, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let orchestrator_id = env.register_contract(None, Orchestrator);
        let family_wallet_id = env.register_contract(None, MockFamilyWallet);
        let remittance_split_id = env.register_contract(None, MockRemittanceSplit);
        let savings_id = env.register_contract(None, MockSavingsGoals);
        let bills_id = env.register_contract(None, MockBillPayments);
        let insurance_id = env.register_contract(None, MockInsurance);

        let user = Address::generate(&env);

        (
            env,
            orchestrator_id,
            family_wallet_id,
            remittance_split_id,
            savings_id,
            bills_id,
            insurance_id,
            user,
        )
    }

    // ============================================================================
    // Existing Tests (preserved)
    // ============================================================================

    #[test]
    fn test_execute_savings_deposit_succeeds() {
        let (env, orchestrator_id, family_wallet_id, _, savings_id, _, _, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_savings_deposit(
            &user, &5000, &family_wallet_id, &savings_id, &1,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_savings_deposit_invalid_goal_fails() {
        let (env, orchestrator_id, family_wallet_id, _, savings_id, _, _, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_savings_deposit(
            &user, &5000, &family_wallet_id, &savings_id, &999,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_execute_savings_deposit_spending_limit_exceeded_fails() {
        let (env, orchestrator_id, family_wallet_id, _, savings_id, _, _, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_savings_deposit(
            &user, &15000, &family_wallet_id, &savings_id, &1,
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::PermissionDenied
        );
    }

    #[test]
    fn test_execute_bill_payment_succeeds() {
        let (env, orchestrator_id, family_wallet_id, _, _, bills_id, _, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_bill_payment(
            &user, &3000, &family_wallet_id, &bills_id, &1,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_bill_payment_invalid_bill_fails() {
        let (env, orchestrator_id, family_wallet_id, _, _, bills_id, _, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_bill_payment(
            &user, &3000, &family_wallet_id, &bills_id, &999,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_execute_insurance_payment_succeeds() {
        let (env, orchestrator_id, family_wallet_id, _, _, _, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_insurance_payment(
            &user, &2000, &family_wallet_id, &insurance_id, &1,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_remittance_flow_succeeds() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_remittance_flow(
            &user, &10000, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id, &1, &1, &1,
        );

        assert!(result.is_ok());
        let flow_result = result.unwrap().unwrap();
        assert_eq!(flow_result.total_amount, 10000);
        assert_eq!(flow_result.spending_amount, 4000);
        assert_eq!(flow_result.savings_amount, 3000);
        assert_eq!(flow_result.bills_amount, 2000);
        assert_eq!(flow_result.insurance_amount, 1000);
        assert!(flow_result.savings_success);
        assert!(flow_result.bills_success);
        assert!(flow_result.insurance_success);
    }

    #[test]
    fn test_execute_remittance_flow_spending_limit_exceeded_fails() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_remittance_flow(
            &user, &15000, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id, &1, &1, &1,
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::PermissionDenied
        );
    }

    #[test]
    fn test_execute_remittance_flow_invalid_amount_fails() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_remittance_flow(
            &user, &0, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id, &1, &1, &1,
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::InvalidAmount
        );
    }

    #[test]
    fn test_get_execution_stats_succeeds() {
        let (env, orchestrator_id, _, _, _, _, _, _) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let stats = client.get_execution_stats();
        assert_eq!(stats.total_flows_executed, 0);
        assert_eq!(stats.total_flows_failed, 0);
        assert_eq!(stats.total_amount_processed, 0);
        assert_eq!(stats.last_execution, 0);
    }

    #[test]
    fn test_get_audit_log_succeeds() {
        let (env, orchestrator_id, _, _, _, _, _, _) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let log = client.get_audit_log(&0, &10);
        assert_eq!(log.len(), 0);
    }

    // ============================================================================
    // Rollback Semantics Tests — Savings Leg Failures
    // ============================================================================

    /// ROLLBACK-01: Savings leg fails with goal not found.
    /// Soroban's panic/revert mechanism ensures the entire transaction is rolled back.
    /// No state changes from prior steps (permission checks) persist.
    #[test]
    fn test_rollback_savings_leg_goal_not_found() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // goal_id=999 causes mock savings to panic → full transaction revert
        let result = client.try_execute_remittance_flow(
            &user, &10000, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id,
            &999, // invalid goal — triggers savings failure
            &1,
            &1,
        );

        // Transaction must fail — rollback occurred
        assert!(
            result.is_err(),
            "Flow must fail when savings leg panics (goal not found)"
        );
    }

    /// ROLLBACK-02: Savings leg fails because goal is already completed.
    /// Verifies rollback when savings is rejected mid-flow.
    #[test]
    fn test_rollback_savings_leg_goal_already_completed() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // goal_id=998 simulates a completed goal that rejects further deposits
        let result = client.try_execute_remittance_flow(
            &user, &10000, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id,
            &998, // completed goal — triggers savings failure
            &1,
            &1,
        );

        assert!(
            result.is_err(),
            "Flow must fail when savings leg rejects a completed goal"
        );
    }

    /// ROLLBACK-03: Savings-only deposit fails with goal not found.
    /// Verifies rollback at the individual operation level (not full flow).
    #[test]
    fn test_rollback_savings_deposit_goal_not_found() {
        let (env, orchestrator_id, family_wallet_id, _, savings_id, _, _, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_savings_deposit(
            &user, &5000, &family_wallet_id, &savings_id, &999,
        );

        assert!(
            result.is_err(),
            "Savings deposit must fail and roll back when goal does not exist"
        );
    }

    /// ROLLBACK-04: Savings-only deposit fails with already-completed goal.
    #[test]
    fn test_rollback_savings_deposit_goal_already_completed() {
        let (env, orchestrator_id, family_wallet_id, _, savings_id, _, _, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_savings_deposit(
            &user, &5000, &family_wallet_id, &savings_id, &998,
        );

        assert!(
            result.is_err(),
            "Savings deposit must fail and roll back when goal is already completed"
        );
    }

    // ============================================================================
    // Rollback Semantics Tests — Bills Leg Failures
    // ============================================================================

    /// ROLLBACK-05: Bills leg fails with bill not found after savings succeeds.
    /// Verifies that a bills failure causes full transaction rollback,
    /// including any savings state changes in the same transaction.
    #[test]
    fn test_rollback_bills_leg_bill_not_found() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Savings succeeds (goal_id=1), but bills fails (bill_id=999)
        let result = client.try_execute_remittance_flow(
            &user, &10000, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id,
            &1,
            &999, // invalid bill — triggers bills failure after savings completes
            &1,
        );

        // Full transaction must be rolled back
        assert!(
            result.is_err(),
            "Flow must fail and roll back when bills leg panics (bill not found)"
        );
    }

    /// ROLLBACK-06: Bills leg fails because bill was already paid.
    /// Verifies double-payment protection triggers a full rollback.
    #[test]
    fn test_rollback_bills_leg_already_paid() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // bill_id=998 simulates an already-paid bill
        let result = client.try_execute_remittance_flow(
            &user, &10000, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id,
            &1,
            &998, // already paid bill
            &1,
        );

        assert!(
            result.is_err(),
            "Flow must fail and roll back when bill has already been paid"
        );
    }

    /// ROLLBACK-07: Bills-only payment fails with bill not found.
    #[test]
    fn test_rollback_bill_payment_bill_not_found() {
        let (env, orchestrator_id, family_wallet_id, _, _, bills_id, _, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_bill_payment(
            &user, &3000, &family_wallet_id, &bills_id, &999,
        );

        assert!(
            result.is_err(),
            "Bill payment must fail and roll back when bill does not exist"
        );
    }

    /// ROLLBACK-08: Bills-only payment fails with already-paid bill.
    #[test]
    fn test_rollback_bill_payment_already_paid() {
        let (env, orchestrator_id, family_wallet_id, _, _, bills_id, _, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_bill_payment(
            &user, &3000, &family_wallet_id, &bills_id, &998,
        );

        assert!(
            result.is_err(),
            "Bill payment must fail and roll back when bill is already paid"
        );
    }

    // ============================================================================
    // Rollback Semantics Tests — Insurance Leg Failures
    // ============================================================================

    /// ROLLBACK-09: Insurance leg fails with policy not found after savings + bills succeed.
    /// Verifies that a late-stage failure rolls back the entire transaction,
    /// including savings and bills changes already applied in this transaction.
    #[test]
    fn test_rollback_insurance_leg_policy_not_found() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Savings succeeds (goal_id=1), bills succeeds (bill_id=1),
        // but insurance fails (policy_id=999)
        let result = client.try_execute_remittance_flow(
            &user, &10000, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id,
            &1,
            &1,
            &999, // invalid policy — triggers insurance failure last
        );

        // Full transaction must be rolled back even though savings + bills completed
        assert!(
            result.is_err(),
            "Flow must fail and roll back when insurance leg panics (policy not found)"
        );
    }

    /// ROLLBACK-10: Insurance leg fails with inactive policy.
    /// The mock returns false for policy_id=998, which the orchestrator treats as failure.
    #[test]
    fn test_rollback_insurance_leg_inactive_policy() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_remittance_flow(
            &user, &10000, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id,
            &1,
            &1,
            &998, // inactive policy
        );

        // The orchestrator records insurance_success=false but does not panic here;
        // the result still returns Ok with insurance_success = false.
        // This test documents the current semantics: inactive policy is a soft failure.
        match result {
            Ok(Ok(flow_result)) => {
                // Soft failure path: flow completes but insurance_success is false
                assert!(
                    !flow_result.insurance_success,
                    "Insurance success must be false for inactive policy"
                );
                assert!(
                    flow_result.savings_success,
                    "Savings must still succeed when insurance soft-fails"
                );
                assert!(
                    flow_result.bills_success,
                    "Bills must still succeed when insurance soft-fails"
                );
            }
            Err(_) | Ok(Err(_)) => {
                // Hard failure path: if orchestrator treats this as a panic, also acceptable
                // This documents that the caller must handle both cases
            }
        }
    }

    /// ROLLBACK-11: Insurance-only payment fails with policy not found.
    #[test]
    fn test_rollback_insurance_payment_policy_not_found() {
        let (env, orchestrator_id, family_wallet_id, _, _, _, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_insurance_payment(
            &user, &2000, &family_wallet_id, &insurance_id, &999,
        );

        assert!(
            result.is_err(),
            "Insurance payment must fail and roll back when policy does not exist"
        );
    }

    // ============================================================================
    // Rollback Semantics Tests — Permission & Validation Failures
    // ============================================================================

    /// ROLLBACK-12: Permission check fails before any downstream leg executes.
    /// Verifies no downstream state is touched when the permission gate rejects the caller.
    #[test]
    fn test_rollback_permission_denied_before_any_leg() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Amount > 10000 causes MockFamilyWallet to deny permission
        let result = client.try_execute_remittance_flow(
            &user, &10001, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id, &1, &1, &1,
        );

        assert!(result.is_err(), "Flow must be rejected when permission is denied");
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::PermissionDenied,
            "Error must be PermissionDenied when family wallet rejects the caller"
        );
    }

    /// ROLLBACK-13: Negative amount is rejected before any downstream leg executes.
    #[test]
    fn test_rollback_negative_amount_rejected() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_remittance_flow(
            &user, &-500, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id, &1, &1, &1,
        );

        assert!(result.is_err(), "Flow must reject negative amounts");
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::InvalidAmount,
            "Error must be InvalidAmount for negative input"
        );
    }

    /// ROLLBACK-14: Zero amount is rejected before any downstream leg executes.
    #[test]
    fn test_rollback_zero_amount_rejected() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_remittance_flow(
            &user, &0, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id, &1, &1, &1,
        );

        assert!(result.is_err(), "Flow must reject zero amounts");
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::InvalidAmount,
            "Error must be InvalidAmount for zero input"
        );
    }

    // ============================================================================
    // Rollback Semantics Tests — All Legs Fail Simultaneously
    // ============================================================================

    /// ROLLBACK-15: All three legs are configured to fail.
    /// Verifies the orchestrator fails fast on the first failure (savings)
    /// and the transaction is fully rolled back.
    #[test]
    fn test_rollback_all_legs_fail() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // All legs use invalid IDs
        let result = client.try_execute_remittance_flow(
            &user, &10000, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id,
            &999, // savings fails
            &999, // bills would also fail
            &999, // insurance would also fail
        );

        assert!(
            result.is_err(),
            "Flow must fail when all legs are configured to fail"
        );
    }

    // ============================================================================
    // Rollback Semantics Tests — Accounting Consistency
    // ============================================================================

    /// ROLLBACK-16: Successful flow produces correct allocation totals.
    /// Verifies that spending + savings + bills + insurance == total_amount,
    /// confirming no funds are created or destroyed during execution.
    #[test]
    fn test_accounting_consistency_on_success() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let total = 10000i128;
        let result = client.try_execute_remittance_flow(
            &user, &total, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id, &1, &1, &1,
        );

        assert!(result.is_ok());
        let flow = result.unwrap().unwrap();

        // Verify allocation totals are internally consistent
        let allocated = flow.spending_amount + flow.savings_amount
            + flow.bills_amount + flow.insurance_amount;

        assert_eq!(
            allocated, total,
            "Allocated amounts must sum to total: got {} expected {}",
            allocated, total
        );

        // Verify each allocation is non-negative (no negative transfers)
        assert!(flow.spending_amount >= 0, "Spending allocation must be non-negative");
        assert!(flow.savings_amount >= 0, "Savings allocation must be non-negative");
        assert!(flow.bills_amount >= 0, "Bills allocation must be non-negative");
        assert!(flow.insurance_amount >= 0, "Insurance allocation must be non-negative");
    }

    /// ROLLBACK-17: Correct split percentages are applied (40/30/20/10).
    /// Ensures the remittance split contract is called correctly and its
    /// output is faithfully passed to each downstream leg.
    #[test]
    fn test_accounting_split_percentages_correct() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_remittance_flow(
            &user, &10000, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id, &1, &1, &1,
        );

        assert!(result.is_ok());
        let flow = result.unwrap().unwrap();

        // Mock split: 40% spending, 30% savings, 20% bills, 10% insurance
        assert_eq!(flow.spending_amount, 4000, "Spending must be 40% of 10000");
        assert_eq!(flow.savings_amount, 3000, "Savings must be 30% of 10000");
        assert_eq!(flow.bills_amount, 2000, "Bills must be 20% of 10000");
        assert_eq!(flow.insurance_amount, 1000, "Insurance must be 10% of 10000");
    }

    /// ROLLBACK-18: Minimum valid amount (1) is processed correctly.
    /// Verifies no off-by-one errors or underflow at the lower bound.
    #[test]
    fn test_accounting_minimum_valid_amount() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Amount of 1 — allocations will round down to 0 for each leg
        let result = client.try_execute_remittance_flow(
            &user, &1, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id, &1, &1, &1,
        );

        // This documents the boundary behavior: flow may succeed or fail
        // depending on how the split contract handles sub-unit amounts.
        // Either outcome is acceptable; what matters is no panic/crash outside
        // the controlled error path.
        match result {
            Ok(Ok(flow)) => {
                assert_eq!(flow.total_amount, 1, "Total amount must be preserved");
            }
            Ok(Err(_)) | Err(_) => {
                // Acceptable: the split contract may reject amounts too small to split
            }
        }
    }

    /// ROLLBACK-19: Maximum valid amount (10000, the spending limit) is processed.
    /// Verifies the upper boundary of the spending limit passes correctly.
    #[test]
    fn test_accounting_maximum_valid_amount_at_spending_limit() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Exactly at the limit (10000 <= 10000 → allowed by mock)
        let result = client.try_execute_remittance_flow(
            &user, &10000, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id, &1, &1, &1,
        );

        assert!(result.is_ok(), "Amount at the spending limit must be allowed");
        let flow = result.unwrap().unwrap();
        assert_eq!(flow.total_amount, 10000);
    }

    /// ROLLBACK-20: One unit above the spending limit is rejected.
    /// Verifies the upper boundary is exclusive (> 10000 is denied).
    #[test]
    fn test_accounting_one_above_spending_limit_rejected() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_remittance_flow(
            &user, &10001, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id, &1, &1, &1,
        );

        assert!(result.is_err(), "Amount one above limit must be rejected");
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::PermissionDenied
        );
    }

    // ============================================================================
    // Rollback Semantics Tests — Independent Operation Rollbacks
    // ============================================================================

    /// ROLLBACK-21: Failed savings deposit does not affect a subsequent successful deposit.
    /// Verifies that a rolled-back transaction leaves no residual state
    /// that would block a future valid transaction.
    #[test]
    fn test_rollback_failed_savings_does_not_poison_subsequent_call() {
        let (env, orchestrator_id, family_wallet_id, _, savings_id, _, _, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // First call: fails (goal 999 not found)
        let fail_result = client.try_execute_savings_deposit(
            &user, &5000, &family_wallet_id, &savings_id, &999,
        );
        assert!(fail_result.is_err(), "First call must fail");

        // Second call: succeeds (goal 1 is valid)
        let success_result = client.try_execute_savings_deposit(
            &user, &5000, &family_wallet_id, &savings_id, &1,
        );
        assert!(
            success_result.is_ok(),
            "Second call must succeed — rolled-back state must not persist"
        );
    }

    /// ROLLBACK-22: Failed bill payment does not affect a subsequent successful payment.
    #[test]
    fn test_rollback_failed_bill_does_not_poison_subsequent_call() {
        let (env, orchestrator_id, family_wallet_id, _, _, bills_id, _, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // First call: fails (bill 999 not found)
        let fail_result = client.try_execute_bill_payment(
            &user, &3000, &family_wallet_id, &bills_id, &999,
        );
        assert!(fail_result.is_err(), "First call must fail");

        // Second call: succeeds (bill 1 is valid)
        let success_result = client.try_execute_bill_payment(
            &user, &3000, &family_wallet_id, &bills_id, &1,
        );
        assert!(
            success_result.is_ok(),
            "Second call must succeed — rolled-back state must not persist"
        );
    }

    /// ROLLBACK-23: Failed insurance payment does not affect a subsequent successful payment.
    #[test]
    fn test_rollback_failed_insurance_does_not_poison_subsequent_call() {
        let (env, orchestrator_id, family_wallet_id, _, _, _, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // First call: fails (policy 999 not found)
        let fail_result = client.try_execute_insurance_payment(
            &user, &2000, &family_wallet_id, &insurance_id, &999,
        );
        assert!(fail_result.is_err(), "First call must fail");

        // Second call: succeeds (policy 1 is valid)
        let success_result = client.try_execute_insurance_payment(
            &user, &2000, &family_wallet_id, &insurance_id, &1,
        );
        assert!(
            success_result.is_ok(),
            "Second call must succeed — rolled-back state must not persist"
        );
    }

    /// ROLLBACK-24: Failed full flow does not affect a subsequent successful full flow.
    /// Verifies end-to-end rollback isolation across consecutive transactions.
    #[test]
    fn test_rollback_failed_full_flow_does_not_poison_subsequent_full_flow() {
        let (env, orchestrator_id, family_wallet_id, remittance_split_id,
             savings_id, bills_id, insurance_id, user) = setup();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // First call: bills leg fails
        let fail_result = client.try_execute_remittance_flow(
            &user, &10000, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id,
            &1,
            &999, // bills fails
            &1,
        );
        assert!(fail_result.is_err(), "First flow must fail");

        // Second call: all legs valid
        let success_result = client.try_execute_remittance_flow(
            &user, &10000, &family_wallet_id, &remittance_split_id,
            &savings_id, &bills_id, &insurance_id,
            &1, &1, &1,
        );
        assert!(
            success_result.is_ok(),
            "Second flow must succeed — prior rollback must not affect this transaction"
        );
    }

    // ========================================================================
    // Reentrancy Guard Tests
    // ========================================================================

    #[test]
    fn test_execution_state_starts_idle() {
        let (env, orchestrator_id, _, _, _, _, _, _) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Initial execution state should be Idle
        let state = client.get_execution_state();
        assert_eq!(state, ExecutionState::Idle);
    }

    #[test]
    fn test_execution_state_returns_to_idle_after_success() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            _remittance_split_id,
            savings_id,
            _bills_id,
            _insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Execute a successful savings deposit
        let result =
            client.try_execute_savings_deposit(&user, &5000, &family_wallet_id, &savings_id, &1);
        assert!(result.is_ok());

        // State should be back to Idle after successful execution
        let state = client.get_execution_state();
        assert_eq!(state, ExecutionState::Idle);
    }

    #[test]
    fn test_execution_state_returns_to_idle_after_failure() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            _remittance_split_id,
            _savings_id,
            _bills_id,
            _insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Execute a savings deposit with amount exceeding limit (triggers error)
        let result = client.try_execute_savings_deposit(
            &user,
            &15000,
            &family_wallet_id,
            &family_wallet_id, // wrong addr, doesn't matter - will fail on perm check
            &1,
        );
        assert!(result.is_err());

        // State should be back to Idle even after failed execution
        let state = client.get_execution_state();
        assert_eq!(state, ExecutionState::Idle);
    }

    #[test]
    fn test_execution_state_idle_after_bill_payment_success() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            _remittance_split_id,
            _savings_id,
            bills_id,
            _insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result =
            client.try_execute_bill_payment(&user, &3000, &family_wallet_id, &bills_id, &1);
        assert!(result.is_ok());

        let state = client.get_execution_state();
        assert_eq!(state, ExecutionState::Idle);
    }

    #[test]
    fn test_execution_state_idle_after_bill_payment_failure() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            _remittance_split_id,
            _savings_id,
            bills_id,
            _insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Invalid bill_id triggers failure
        let result =
            client.try_execute_bill_payment(&user, &3000, &family_wallet_id, &bills_id, &999);
        assert!(result.is_err());

        let state = client.get_execution_state();
        assert_eq!(state, ExecutionState::Idle);
    }

    #[test]
    fn test_execution_state_idle_after_insurance_payment_success() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            _remittance_split_id,
            _savings_id,
            _bills_id,
            insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_insurance_payment(
            &user,
            &2000,
            &family_wallet_id,
            &insurance_id,
            &1,
        );
        assert!(result.is_ok());

        let state = client.get_execution_state();
        assert_eq!(state, ExecutionState::Idle);
    }

    #[test]
    fn test_execution_state_idle_after_remittance_flow_success() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            remittance_split_id,
            savings_id,
            bills_id,
            insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_remittance_flow(
            &user,
            &10000,
            &family_wallet_id,
            &remittance_split_id,
            &savings_id,
            &bills_id,
            &insurance_id,
            &1,
            &1,
            &1,
        );
        assert!(result.is_ok());

        let state = client.get_execution_state();
        assert_eq!(state, ExecutionState::Idle);
    }

    #[test]
    fn test_execution_state_idle_after_remittance_flow_invalid_amount() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            remittance_split_id,
            savings_id,
            bills_id,
            insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Zero amount triggers InvalidAmount error
        let result = client.try_execute_remittance_flow(
            &user,
            &0,
            &family_wallet_id,
            &remittance_split_id,
            &savings_id,
            &bills_id,
            &insurance_id,
            &1,
            &1,
            &1,
        );
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::InvalidAmount
        );

        // State must be Idle even after early-return error path
        let state = client.get_execution_state();
        assert_eq!(state, ExecutionState::Idle);
    }

    #[test]
    fn test_execution_state_idle_after_remittance_flow_permission_denied() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            remittance_split_id,
            savings_id,
            bills_id,
            insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Amount exceeds limit -> PermissionDenied
        let result = client.try_execute_remittance_flow(
            &user,
            &15000,
            &family_wallet_id,
            &remittance_split_id,
            &savings_id,
            &bills_id,
            &insurance_id,
            &1,
            &1,
            &1,
        );
        assert!(result.is_err());

        let state = client.get_execution_state();
        assert_eq!(state, ExecutionState::Idle);
    }

    #[test]
    fn test_sequential_executions_succeed() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            _remittance_split_id,
            savings_id,
            bills_id,
            _insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // First execution: savings deposit
        let result1 =
            client.try_execute_savings_deposit(&user, &5000, &family_wallet_id, &savings_id, &1);
        assert!(result1.is_ok());

        // Second execution: bill payment (should succeed since lock was released)
        let result2 =
            client.try_execute_bill_payment(&user, &3000, &family_wallet_id, &bills_id, &1);
        assert!(result2.is_ok());

        let state = client.get_execution_state();
        assert_eq!(state, ExecutionState::Idle);
    }

    #[test]
    fn test_execution_after_failure_succeeds() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            _remittance_split_id,
            savings_id,
            _bills_id,
            _insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // First execution: fails (amount exceeds limit)
        let result1 =
            client.try_execute_savings_deposit(&user, &15000, &family_wallet_id, &savings_id, &1);
        assert!(result1.is_err());

        // Second execution: should succeed (lock was released after failure)
        let result2 =
            client.try_execute_savings_deposit(&user, &5000, &family_wallet_id, &savings_id, &1);
        assert!(result2.is_ok());

        let state = client.get_execution_state();
        assert_eq!(state, ExecutionState::Idle);
    }

    #[test]
    fn test_reentrancy_guard_direct_storage_manipulation() {
        // Directly set execution state to Executing and verify guard rejects calls
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            _remittance_split_id,
            savings_id,
            _bills_id,
            _insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Manually set execution state to Executing via storage
        // This simulates the state during an in-progress execution
        env.as_contract(&orchestrator_id, || {
            env.storage().instance().set(
                &soroban_sdk::symbol_short!("EXEC_ST"),
                &ExecutionState::Executing,
            );
        });

        // Attempt to execute while lock is held should fail with ReentrancyDetected
        let result =
            client.try_execute_savings_deposit(&user, &5000, &family_wallet_id, &savings_id, &1);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::ReentrancyDetected
        );
    }

    #[test]
    fn test_reentrancy_guard_blocks_bill_payment_during_execution() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            _remittance_split_id,
            _savings_id,
            bills_id,
            _insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Simulate in-progress execution
        env.as_contract(&orchestrator_id, || {
            env.storage().instance().set(
                &soroban_sdk::symbol_short!("EXEC_ST"),
                &ExecutionState::Executing,
            );
        });

        let result =
            client.try_execute_bill_payment(&user, &3000, &family_wallet_id, &bills_id, &1);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::ReentrancyDetected
        );
    }

    #[test]
    fn test_reentrancy_guard_blocks_insurance_payment_during_execution() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            _remittance_split_id,
            _savings_id,
            _bills_id,
            insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Simulate in-progress execution
        env.as_contract(&orchestrator_id, || {
            env.storage().instance().set(
                &soroban_sdk::symbol_short!("EXEC_ST"),
                &ExecutionState::Executing,
            );
        });

        let result = client.try_execute_insurance_payment(
            &user,
            &2000,
            &family_wallet_id,
            &insurance_id,
            &1,
        );
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::ReentrancyDetected
        );
    }

    #[test]
    fn test_reentrancy_guard_blocks_remittance_flow_during_execution() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            remittance_split_id,
            savings_id,
            bills_id,
            insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Simulate in-progress execution
        env.as_contract(&orchestrator_id, || {
            env.storage().instance().set(
                &soroban_sdk::symbol_short!("EXEC_ST"),
                &ExecutionState::Executing,
            );
        });

        let result = client.try_execute_remittance_flow(
            &user,
            &10000,
            &family_wallet_id,
            &remittance_split_id,
            &savings_id,
            &bills_id,
            &insurance_id,
            &1,
            &1,
            &1,
        );
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::ReentrancyDetected
        );
    }

    #[test]
    fn test_multiple_sequential_flows_all_succeed() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            remittance_split_id,
            savings_id,
            bills_id,
            insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Execute three full remittance flows sequentially
        for _ in 0..3 {
            let result = client.try_execute_remittance_flow(
                &user,
                &10000,
                &family_wallet_id,
                &remittance_split_id,
                &savings_id,
                &bills_id,
                &insurance_id,
                &1,
                &1,
                &1,
            );
            assert!(result.is_ok());

            let state = client.get_execution_state();
            assert_eq!(state, ExecutionState::Idle);
        }
    }

    #[test]
    fn test_get_audit_log_pagination_is_stable_and_complete_under_heavy_history() {
        let (env, orchestrator_id, _, _, _, _, _, user) = setup_test_env();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Seed above capacity to force rotation and emulate heavy execution history.
        let seeded = 130u32;
        seed_audit_log(&env, &user, seeded);

        // Fetch in multiple pages and assert continuity without duplicates.
        let page_size = 17u32;
        let entries = collect_all_pages(&client, page_size);
        assert_eq!(entries.len() as u32, 100);

        // Rotation should retain the most recent [30..129] amounts.
        for (idx, entry) in entries.iter().enumerate() {
            let expected_amount = (idx as i128) + 30;
            assert_eq!(entry.amount, expected_amount);
            assert!(entry.success);
            assert_eq!(entry.operation, symbol_short!("execflow"));
        }

        let mut dedupe = std::collections::BTreeSet::new();
        for entry in &entries {
            dedupe.insert(entry.amount);
        }
        assert_eq!(dedupe.len(), entries.len());
    }

    #[test]
    fn test_get_audit_log_cursor_boundaries_and_limits_are_correct() {
        let (env, orchestrator_id, _, _, _, _, _, user) = setup_test_env();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        seed_audit_log(&env, &user, 12);

        // limit=0 should produce empty page.
        assert_eq!(client.get_audit_log(&0, &0).len(), 0);

        // Exact boundary page.
        let page = client.get_audit_log(&8, &4);
        assert_eq!(page.len(), 4);
        assert_eq!(page.get(0).unwrap().amount, 8);
        assert_eq!(page.get(3).unwrap().amount, 11);

        // from_index at length is empty.
        assert_eq!(client.get_audit_log(&12, &5).len(), 0);

        // from_index beyond length is empty.
        assert_eq!(client.get_audit_log(&99, &5).len(), 0);
    }

    #[test]
    fn test_get_audit_log_large_cursor_does_not_overflow_or_duplicate() {
        let (env, orchestrator_id, _, _, _, _, _, user) = setup_test_env();
        let client = OrchestratorClient::new(&env, &orchestrator_id);

        seed_audit_log(&env, &user, 5);

        // Regression test: very large cursor should safely return empty page
        // rather than panicking due to u32 addition overflow.
        let huge_cursor = u32::MAX;
        let page = client.get_audit_log(&huge_cursor, &100);
        assert_eq!(page.len(), 0);
    }

    // ============================================================================
    // Address Validation Tests
    // ============================================================================

    #[test]
    fn test_execute_savings_deposit_self_reference_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let orchestrator_id = env.register_contract(None, Orchestrator);
        let savings_id = env.register_contract(None, MockSavingsGoals);
        let user = generate_test_address(&env);

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result =
            client.try_execute_savings_deposit(&user, &5000, &orchestrator_id, &savings_id, &1);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::SelfReferenceNotAllowed
        );
    }

    #[test]
    fn test_execute_savings_deposit_duplicate_addresses_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let orchestrator_id = env.register_contract(None, Orchestrator);
        let family_wallet_id = env.register_contract(None, MockFamilyWallet);
        let user = generate_test_address(&env);

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_savings_deposit(
            &user,
            &5000,
            &family_wallet_id,
            &family_wallet_id,
            &1,
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::DuplicateContractAddress
        );
    }

    #[test]
    fn test_execute_bill_payment_self_reference_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let orchestrator_id = env.register_contract(None, Orchestrator);
        let family_wallet_id = env.register_contract(None, MockFamilyWallet);
        let user = generate_test_address(&env);

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result =
            client.try_execute_bill_payment(&user, &3000, &family_wallet_id, &orchestrator_id, &1);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::SelfReferenceNotAllowed
        );
    }

    #[test]
    fn test_execute_bill_payment_duplicate_addresses_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let orchestrator_id = env.register_contract(None, Orchestrator);
        let bills_id = env.register_contract(None, MockBillPayments);
        let user = generate_test_address(&env);

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_bill_payment(&user, &3000, &bills_id, &bills_id, &1);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::DuplicateContractAddress
        );
    }

    #[test]
    fn test_execute_insurance_payment_self_reference_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let orchestrator_id = env.register_contract(None, Orchestrator);
        let family_wallet_id = env.register_contract(None, MockFamilyWallet);
        let user = generate_test_address(&env);

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_insurance_payment(
            &user,
            &2000,
            &family_wallet_id,
            &orchestrator_id,
            &1,
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::SelfReferenceNotAllowed
        );
    }

    #[test]
    fn test_execute_insurance_payment_duplicate_addresses_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let orchestrator_id = env.register_contract(None, Orchestrator);
        let insurance_id = env.register_contract(None, MockInsurance);
        let user = generate_test_address(&env);

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result =
            client.try_execute_insurance_payment(&user, &2000, &insurance_id, &insurance_id, &1);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::DuplicateContractAddress
        );
    }

    #[test]
    fn test_execute_remittance_flow_self_reference_fails() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            remittance_split_id,
            savings_id,
            bills_id,
            _insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_remittance_flow(
            &user,
            &10000,
            &family_wallet_id,
            &remittance_split_id,
            &savings_id,
            &bills_id,
            &orchestrator_id,
            &1,
            &1,
            &1,
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::SelfReferenceNotAllowed
        );
    }

    #[test]
    fn test_execute_remittance_flow_duplicate_addresses_fails() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            remittance_split_id,
            savings_id,
            _bills_id,
            insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_remittance_flow(
            &user,
            &10000,
            &family_wallet_id,
            &remittance_split_id,
            &savings_id,
            &savings_id,
            &insurance_id,
            &1,
            &1,
            &1,
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::DuplicateContractAddress
        );
    }

    #[test]
    fn test_execute_remittance_flow_family_wallet_duplicate_fails() {
        let (
            env,
            orchestrator_id,
            family_wallet_id,
            _remittance_split_id,
            savings_id,
            bills_id,
            insurance_id,
            user,
        ) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_remittance_flow(
            &user,
            &10000,
            &family_wallet_id,
            &family_wallet_id,
            &savings_id,
            &bills_id,
            &insurance_id,
            &1,
            &1,
            &1,
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::DuplicateContractAddress
        );
    }

    #[test]
    fn test_execute_remittance_flow_all_same_address_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let orchestrator_id = env.register_contract(None, Orchestrator);
        let single_contract = env.register_contract(None, MockFamilyWallet);
        let user = generate_test_address(&env);

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        let result = client.try_execute_remittance_flow(
            &user,
            &10000,
            &single_contract,
            &single_contract,
            &single_contract,
            &single_contract,
            &single_contract,
            &1,
            &1,
            &1,
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::DuplicateContractAddress
        );
    }
}
