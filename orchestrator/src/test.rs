use crate::{Orchestrator, OrchestratorClient, OrchestratorError};
use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
use testutils::{generate_test_address, setup_test_env};

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
    pub fn add_to_goal(_env: Env, _caller: Address, goal_id: u32, amount: i128) -> i128 {
        if goal_id == 999 {
            panic!("Goal not found");
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
    /// Panics if bill_id == 999 (simulating bill not found or already paid)
    pub fn pay_bill(_env: Env, _caller: Address, bill_id: u32) {
        if bill_id == 999 {
            panic!("Bill not found or already paid");
        }
    }
}

/// Mock Insurance contract for testing
#[contract]
pub struct MockInsurance;

#[contractimpl]
impl MockInsurance {
    /// Mock implementation of pay_premium
    /// Returns false if policy_id == 999 (simulating inactive policy)
    pub fn pay_premium(_env: Env, _caller: Address, policy_id: u32) -> bool {
        policy_id != 999
    }
}

// ============================================================================
// Integration Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Set up test environment with all contracts deployed
    fn setup_test_env() -> (
        Env,
        Address,
        Address,
        Address,
        Address,
        Address,
        Address,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        // Register and deploy all contracts
        let orchestrator_id = env.register_contract(None, Orchestrator);
        let family_wallet_id = env.register_contract(None, MockFamilyWallet);
        let remittance_split_id = env.register_contract(None, MockRemittanceSplit);
        let savings_id = env.register_contract(None, MockSavingsGoals);
        let bills_id = env.register_contract(None, MockBillPayments);
        let insurance_id = env.register_contract(None, MockInsurance);

        // Create test user address
        let user = generate_test_address(&env);

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

    #[test]
    fn test_execute_savings_deposit_succeeds() {
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

        // Execute savings deposit with amount within limit (5000 <= 10000)
        let result = client.try_execute_savings_deposit(
            &user,
            &5000,
            &family_wallet_id,
            &savings_id,
            &1, // goal_id
        );

        // Should succeed
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_savings_deposit_invalid_goal_fails() {
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

        // Execute savings deposit with invalid goal_id (999)
        // This should fail with SavingsDepositFailed error
        let result = client.try_execute_savings_deposit(
            &user,
            &5000,
            &family_wallet_id,
            &savings_id,
            &999, // invalid goal_id
        );

        // Should fail (the mock will panic, which gets caught and converted to error)
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_savings_deposit_spending_limit_exceeded_fails() {
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

        // Execute savings deposit with amount exceeding limit (15000 > 10000)
        let result = client.try_execute_savings_deposit(
            &user,
            &15000,
            &family_wallet_id,
            &savings_id,
            &1, // goal_id
        );

        // Should fail - the mock returns false for amounts > 10000
        // This gets interpreted as PermissionDenied (since check_spending_limit
        // and check_family_wallet_permission use the same mock function)
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::PermissionDenied
        );
    }

    #[test]
    fn test_execute_bill_payment_succeeds() {
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

        // Execute bill payment with amount within limit
        let result = client.try_execute_bill_payment(
            &user,
            &3000,
            &family_wallet_id,
            &bills_id,
            &1, // bill_id
        );

        // Should succeed
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_bill_payment_invalid_bill_fails() {
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

        // Execute bill payment with invalid bill_id (999)
        // This should fail with BillPaymentFailed error
        let result = client.try_execute_bill_payment(
            &user,
            &3000,
            &family_wallet_id,
            &bills_id,
            &999, // invalid bill_id
        );

        // Should fail (the mock will panic, which gets caught and converted to error)
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_insurance_payment_succeeds() {
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

        // Execute insurance payment with amount within limit
        let result = client.try_execute_insurance_payment(
            &user,
            &2000,
            &family_wallet_id,
            &insurance_id,
            &1, // policy_id
        );

        // Should succeed
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_remittance_flow_succeeds() {
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

        // Execute complete remittance flow with amount within limit (10000)
        let result = client.try_execute_remittance_flow(
            &user,
            &10000,
            &family_wallet_id,
            &remittance_split_id,
            &savings_id,
            &bills_id,
            &insurance_id,
            &1, // goal_id
            &1, // bill_id
            &1, // policy_id
        );

        // Should succeed
        assert!(result.is_ok());

        let flow_result = result.unwrap().unwrap();

        // Verify allocations (40%, 30%, 20%, 10%)
        assert_eq!(flow_result.total_amount, 10000);
        assert_eq!(flow_result.spending_amount, 4000);
        assert_eq!(flow_result.savings_amount, 3000);
        assert_eq!(flow_result.bills_amount, 2000);
        assert_eq!(flow_result.insurance_amount, 1000);

        // Verify all operations succeeded
        assert!(flow_result.savings_success);
        assert!(flow_result.bills_success);
        assert!(flow_result.insurance_success);
    }

    #[test]
    fn test_execute_remittance_flow_bill_failure_rolls_back() {
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

        // Execute remittance flow with invalid bill_id (999)
        // The mock will panic, but the orchestrator catches it and returns an error
        let result = client.try_execute_remittance_flow(
            &user,
            &10000,
            &family_wallet_id,
            &remittance_split_id,
            &savings_id,
            &bills_id,
            &insurance_id,
            &1,   // valid goal_id
            &999, // invalid bill_id - will cause failure
            &1,   // valid policy_id
        );

        // Should fail (panic gets caught and converted to error)
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_remittance_flow_savings_failure_rolls_back() {
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

        // Execute remittance flow with invalid goal_id (999)
        // The mock will panic, but the orchestrator catches it and returns an error
        let result = client.try_execute_remittance_flow(
            &user,
            &10000,
            &family_wallet_id,
            &remittance_split_id,
            &savings_id,
            &bills_id,
            &insurance_id,
            &999, // invalid goal_id - will cause failure
            &1,   // valid bill_id
            &1,   // valid policy_id
        );

        // Should fail (panic gets caught and converted to error)
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_remittance_flow_spending_limit_exceeded_fails() {
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

        // Execute remittance flow with amount exceeding limit (15000 > 10000)
        let result = client.try_execute_remittance_flow(
            &user,
            &15000,
            &family_wallet_id,
            &remittance_split_id,
            &savings_id,
            &bills_id,
            &insurance_id,
            &1, // goal_id
            &1, // bill_id
            &1, // policy_id
        );

        // Should fail - the mock returns false for amounts > 10000
        // This gets interpreted as PermissionDenied (since check_spending_limit
        // and check_family_wallet_permission use the same mock function)
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::PermissionDenied
        );
    }

    #[test]
    fn test_execute_remittance_flow_invalid_amount_fails() {
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

        // Execute remittance flow with invalid amount (0)
        let result = client.try_execute_remittance_flow(
            &user,
            &0, // invalid amount
            &family_wallet_id,
            &remittance_split_id,
            &savings_id,
            &bills_id,
            &insurance_id,
            &1, // goal_id
            &1, // bill_id
            &1, // policy_id
        );

        // Should fail with InvalidAmount
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            OrchestratorError::InvalidAmount
        );
    }

    #[test]
    fn test_get_execution_stats_succeeds() {
        let (env, orchestrator_id, _, _, _, _, _, _) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Get initial stats (should be all zeros)
        let stats = client.get_execution_stats();

        assert_eq!(stats.total_flows_executed, 0);
        assert_eq!(stats.total_flows_failed, 0);
        assert_eq!(stats.total_amount_processed, 0);
        assert_eq!(stats.last_execution, 0);
    }

    #[test]
    fn test_get_audit_log_succeeds() {
        let (env, orchestrator_id, _, _, _, _, _, _) = setup_test_env();

        let client = OrchestratorClient::new(&env, &orchestrator_id);

        // Get audit log (should be empty initially)
        let log = client.get_audit_log(&0, &10);

        assert_eq!(log.len(), 0);
    }
}
