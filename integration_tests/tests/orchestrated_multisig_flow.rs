use bill_payments::{BillPayments, BillPaymentsClient};
use family_wallet::{FamilyWallet, FamilyWalletClient, TransactionData, TransactionType};
use insurance::{Insurance, InsuranceClient};
use orchestrator::{Orchestrator, OrchestratorClient, OrchestratorError};
use remittance_split::{RemittanceSplit, RemittanceSplitClient};
use remitwise_common::{CoverageType, FamilyRole};
use reporting::{ReportingContract, ReportingContractClient};
use savings_goals::{SavingsGoalContract, SavingsGoalContractClient};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{symbol_short, Address, Env, String as SorobanString};

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

/// Integration test for the orchestrated multisig flow.
///
/// This test validates:
/// 1. Registration and initialization of all involved contracts.
/// 2. Multisig configuration in FamilyWallet.
/// 3. Orchestrator flow is gated by FamilyWallet spending limits (simulated quorum).
/// 4. Quorum reaching via propose_transaction and sign_transaction.
/// 5. Paused contract behavior.
/// 6. EXEC_LOCK reentrancy protection.
#[test]
fn test_orchestrated_multisig_flow() {
    let env = make_env();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);

    // 1. Register contracts
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let orchestrator_client = OrchestratorClient::new(&env, &orchestrator_id);

    let remittance_id = env.register_contract(None, RemittanceSplit);
    let remittance_client = RemittanceSplitClient::new(&env, &remittance_id);

    let savings_id = env.register_contract(None, SavingsGoalContract);
    let savings_client = SavingsGoalContractClient::new(&env, &savings_id);

    let bills_id = env.register_contract(None, BillPayments);
    let bills_client = BillPaymentsClient::new(&env, &bills_id);

    let insurance_id = env.register_contract(None, Insurance);
    let insurance_client = InsuranceClient::new(&env, &insurance_id);

    let family_wallet_id = env.register_contract(None, FamilyWallet);
    let family_wallet_client = FamilyWalletClient::new(&env, &family_wallet_id);

    let reporting_id = env.register_contract(None, ReportingContract);
    let reporting_client = ReportingContractClient::new(&env, &reporting_id);

    // 2. Initialize contracts
    family_wallet_client.init(
        &admin,
        &soroban_sdk::vec![&env, user.clone(), member1.clone(), member2.clone()],
    );

    // Set low spending limit for user to force multisig/role change
    family_wallet_client.update_spending_limit(&admin, &user, &100i128);

    let mock_usdc = Address::generate(&env);
    remittance_client
        .initialize_split(&admin, &0u64, &mock_usdc, &40u32, &30u32, &20u32, &10u32)
        .unwrap();

    savings_client.init();

    reporting_client.init(&admin);
    reporting_client.configure_addresses(
        &admin,
        &remittance_id,
        &savings_id,
        &bills_id,
        &insurance_id,
        &family_wallet_id,
    );

    orchestrator_client.init(
        &admin,
        &family_wallet_id,
        &remittance_id,
        &savings_id,
        &bills_id,
        &insurance_id,
    );

    // Setup goals/bills/policies for the user
    let goal_id = savings_client
        .create_goal(
            &user,
            &SorobanString::from_str(&env, "Test Goal"),
            &10000i128,
            &2000000000u64,
        )
        .unwrap();
    let bill_id = bills_client.create_bill(
        &user,
        &SorobanString::from_str(&env, "Test Bill"),
        &1000i128,
        &2000000000u64,
        &true,
        &30u32,
        &None,
        &SorobanString::from_str(&env, "USDC"),
        &None,
    );
    let policy_id = insurance_client.create_policy(
        &user,
        &SorobanString::from_str(&env, "Test Policy"),
        &CoverageType::Health,
        &100i128,
        &50000i128,
        &None,
    );

    // 3. Scenario: Quorum not met
    /// Scenario: User attempts flow exceeding their limit.
    /// In this system, "quorum" for exceeding limits is handled by role elevation.
    /// Since the user is not yet an Admin, the Orchestrator check fails.
    let total_amount = 5000i128;
    let result = orchestrator_client.try_execute_remittance_flow(
        &RemittanceFlowParams {
            caller: user.clone(),
            total_amount,
            family_wallet: family_wallet_id.clone(),
            remittance_split: remittance_id.clone(),
            savings: savings_id.clone(),
            bills: bills_id.clone(),
            insurance: insurance_id.clone(),
            goal_id,
            bill_id,
            policy_id,
        },
    );

    match result {
        Err(Ok(OrchestratorError::Unauthorized)) => (),
        _ => panic!("Expected Unauthorized error due to spending limit"),
    }

    // 4. Scenario: Reaching Quorum
    /// Scenario: Propose and sign a role change to elevate the user to Admin.
    /// This demonstrates the multisig quorum logic in FamilyWallet.
    family_wallet_client
        .configure_multisig(
            &admin,
            &TransactionType::RoleChange,
            &2, // Threshold of 2
            &soroban_sdk::vec![&env, admin.clone(), member1.clone(), member2.clone()],
            &0,
        )
        .unwrap();

    let tx_id = family_wallet_client.propose_role_change(&admin, &user, &FamilyRole::Admin);

    // Assert flow still fails (quorum not yet met)
    let result_still_fails = orchestrator_client.try_execute_remittance_flow(
        &RemittanceFlowParams {
            caller: user.clone(),
            total_amount,
            family_wallet: family_wallet_id.clone(),
            remittance_split: remittance_id.clone(),
            savings: savings_id.clone(),
            bills: bills_id.clone(),
            insurance: insurance_id.clone(),
            goal_id,
            bill_id,
            policy_id,
        },
    );
    assert!(result_still_fails.is_err());

    // Sign to reach quorum
    family_wallet_client
        .sign_transaction(&member1, &tx_id)
        .unwrap();

    // Now quorum is met, user is Admin, flow should succeed
    orchestrator_client
        .execute_remittance_flow(&RemittanceFlowParams {
            caller: user.clone(),
            total_amount,
            family_wallet: family_wallet_id.clone(),
            remittance_split: remittance_id.clone(),
            savings: savings_id.clone(),
            bills: bills_id.clone(),
            insurance: insurance_id.clone(),
            goal_id,
            bill_id,
            policy_id,
        })
        .unwrap();

    // 5. Scenario: Paused Orchestrator (Downstream contract paused)
    /// Scenario: Pause the SavingsGoalContract.
    /// Since the Orchestrator depends on it, the flow should now fail.
    savings_client.set_pause_admin(&admin, &admin);
    savings_client.pause(&admin);

    let result_paused = orchestrator_client.try_execute_remittance_flow(
        &RemittanceFlowParams {
            caller: user.clone(),
            total_amount,
            family_wallet: family_wallet_id.clone(),
            remittance_split: remittance_id.clone(),
            savings: savings_id.clone(),
            bills: bills_id.clone(),
            insurance: insurance_id.clone(),
            goal_id,
            bill_id,
            policy_id,
        },
    );
    assert!(
        result_paused.is_err(),
        "Flow should fail when a dependency is paused"
    );

    // Unpause for next check
    savings_client.unpause(&admin);

    // 6. Scenario: EXEC_LOCK behavior
    /// Scenario: Manually set the EXEC_LOCK to simulate an active execution.
    /// The Orchestrator should prevent a second concurrent execution.
    env.as_contract(&orchestrator_id, || {
        env.storage()
            .instance()
            .set(&symbol_short!("EXEC_LOCK"), &true);
    });

    let result_locked = orchestrator_client.try_execute_remittance_flow(
        &RemittanceFlowParams {
            caller: user.clone(),
            total_amount,
            family_wallet: family_wallet_id.clone(),
            remittance_split: remittance_id.clone(),
            savings: savings_id.clone(),
            bills: bills_id.clone(),
            insurance: insurance_id.clone(),
            goal_id,
            bill_id,
            policy_id,
        },
    );

    match result_locked {
        Err(Ok(OrchestratorError::ExecutionLocked)) => (),
        _ => panic!("Expected ExecutionLocked error"),
    }
}
