#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, Address, Env, String as SorobanString, Vec,
};

use bill_payments::{BillPayments, BillPaymentsClient};
use insurance::{Insurance, InsuranceClient, InsuranceError};
use orchestrator::{Orchestrator, OrchestratorClient, OrchestratorError};
use remittance_split::{RemittanceSplit, RemittanceSplitClient, RemittanceSplitError};
use remitwise_common::CoverageType;
use savings_goals::{
    GoalsExportSnapshot, SavingsGoalContract, SavingsGoalContractClient, SavingsGoalError,
};

#[contract]
pub struct MockFamilyWallet;

#[contractimpl]
impl MockFamilyWallet {
    pub fn check_spending_limit(_env: Env, _caller: Address, amount: i128) -> bool {
        amount <= 100_000
    }
}

#[contract]
pub struct MockRemittanceSplit;

#[contractimpl]
impl MockRemittanceSplit {
    pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
        let spending = (total_amount * 40) / 100;
        let savings = (total_amount * 30) / 100;
        let bills = (total_amount * 20) / 100;
        let insurance = total_amount - spending - savings - bills;
        Vec::from_array(&env, [spending, savings, bills, insurance])
    }
}

#[contract]
pub struct MockSavingsGoals;

#[contractimpl]
impl MockSavingsGoals {
    pub fn add_to_goal(_env: Env, _caller: Address, goal_id: u32, amount: i128) -> i128 {
        if goal_id == 999 {
            panic!("Goal not found");
        }
        amount
    }
}

#[contract]
pub struct MockBillPayments;

#[contractimpl]
impl MockBillPayments {
    pub fn pay_bill(_env: Env, _caller: Address, bill_id: u32) {
        if bill_id == 999 {
            panic!("Bill not found");
        }
    }
}

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

#[test]
fn test_multi_contract_user_flow_smoke() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);

    let remittance_id = env.register_contract(None, RemittanceSplit);
    let remittance_client = RemittanceSplitClient::new(&env, &remittance_id);

    let savings_id = env.register_contract(None, SavingsGoalContract);
    let savings_client = SavingsGoalContractClient::new(&env, &savings_id);

    let bills_id = env.register_contract(None, BillPayments);
    let bills_client = BillPaymentsClient::new(&env, &bills_id);

    let insurance_id = env.register_contract(None, Insurance);
    let insurance_client = InsuranceClient::new(&env, &insurance_id);

    remittance_client
        .try_initialize_split(
            &user,
            &0u64,
            &Address::generate(&env),
            &40u32,
            &30u32,
            &20u32,
            &10u32,
        )
        .unwrap()
        .unwrap();
    assert_eq!(remittance_client.get_nonce(&user), 1u64);

    savings_client.init();
    let goal_id = savings_client
        .try_create_goal(
            &user,
            &SorobanString::from_str(&env, "Education Fund"),
            &10_000i128,
            &(env.ledger().timestamp() + 365 * 86400),
        )
        .unwrap()
        .unwrap();
    assert_eq!(goal_id, 1u32);

    let bill_id = bills_client
        .try_create_bill(
            &user,
            &SorobanString::from_str(&env, "Electricity Bill"),
            &500i128,
            &(env.ledger().timestamp() + 30 * 86400),
            &true,
            &30u32,
            &None,
            &SorobanString::from_str(&env, "XLM"),
        )
        .unwrap()
        .unwrap();
    assert_eq!(bill_id, 1u32);

    insurance_client.try_initialize(&user).unwrap().unwrap();
    let policy_id = insurance_client
        .try_create_policy(
            &user,
            &SorobanString::from_str(&env, "Health Insurance"),
            &CoverageType::Health,
            &500i128,
            &50_000i128,
            &None,
        )
        .unwrap()
        .unwrap();
    assert_eq!(policy_id, 1u32);

    let total_remittance = 10_000i128;
    let amounts = remittance_client.calculate_split(&total_remittance);
    let spending_amount = amounts.get(0).unwrap();
    let savings_amount = amounts.get(1).unwrap();
    let bills_amount = amounts.get(2).unwrap();
    let insurance_amount = amounts.get(3).unwrap();

    assert_eq!(
        spending_amount + savings_amount + bills_amount + insurance_amount,
        total_remittance
    );
}

#[test]
fn test_orchestrator_nonce_sequential_across_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let wallet_id = env.register_contract(None, MockFamilyWallet);
    let split_id = env.register_contract(None, MockRemittanceSplit);
    let savings_id = env.register_contract(None, MockSavingsGoals);
    let bills_id = env.register_contract(None, MockBillPayments);
    let insurance_id = env.register_contract(None, MockInsurance);

    let client = OrchestratorClient::new(&env, &orchestrator_id);

    assert_eq!(client.get_nonce(&user), 0u64);

    client
        .try_execute_savings_deposit(&user, &10i128, &wallet_id, &savings_id, &1u32, &0u64)
        .unwrap()
        .unwrap();
    assert_eq!(client.get_nonce(&user), 1u64);

    client
        .try_execute_bill_payment(&user, &10i128, &wallet_id, &bills_id, &1u32, &1u64)
        .unwrap()
        .unwrap();
    assert_eq!(client.get_nonce(&user), 2u64);

    client
        .try_execute_insurance_payment(&user, &10i128, &wallet_id, &insurance_id, &1u32, &2u64)
        .unwrap()
        .unwrap();
    assert_eq!(client.get_nonce(&user), 3u64);

    let replay =
        client.try_execute_bill_payment(&user, &10i128, &wallet_id, &bills_id, &1u32, &1u64);
    assert_eq!(replay, Err(Ok(OrchestratorError::InvalidNonce)));

    let bad_nonce =
        client.try_execute_savings_deposit(&user, &10i128, &wallet_id, &savings_id, &1u32, &999u64);
    assert_eq!(bad_nonce, Err(Ok(OrchestratorError::InvalidNonce)));

    let bad_address =
        client.try_execute_bill_payment(&user, &10i128, &wallet_id, &wallet_id, &1u32, &3u64);
    assert_eq!(
        bad_address,
        Err(Ok(OrchestratorError::DuplicateContractAddress))
    );

    let _ = split_id;
}

#[test]
fn test_savings_goals_snapshot_nonce_replay_protection() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);
    let savings_id = env.register_contract(None, SavingsGoalContract);
    let savings_client = SavingsGoalContractClient::new(&env, &savings_id);

    savings_client.init();
    let _ = savings_client
        .try_create_goal(
            &user,
            &SorobanString::from_str(&env, "Snapshot Goal"),
            &1_000i128,
            &(env.ledger().timestamp() + 86400),
        )
        .unwrap()
        .unwrap();

    let snapshot: GoalsExportSnapshot = savings_client.export_snapshot(&user);

    let ok = savings_client.try_import_snapshot(&user, &0u64, &snapshot);
    assert_eq!(ok, Ok(Ok(true)));
    assert_eq!(savings_client.get_nonce(&user), 1u64);

    let replay = savings_client.try_import_snapshot(&user, &0u64, &snapshot);
    assert_eq!(replay, Err(Ok(SavingsGoalError::InvalidNonce)));
}

#[test]
fn test_remittance_split_nonce_replay_protection() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);
    let remittance_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &remittance_id);

    let usdc = Address::generate(&env);
    assert_eq!(
        client.try_initialize_split(&user, &0u64, &usdc, &40u32, &30u32, &20u32, &10u32),
        Ok(Ok(true))
    );
    assert_eq!(client.get_nonce(&user), 1u64);

    let replay = client.try_initialize_split(&user, &0u64, &usdc, &40u32, &30u32, &20u32, &10u32);
    assert_eq!(replay, Err(Ok(RemittanceSplitError::InvalidNonce)));
}

#[test]
fn test_insurance_try_create_policy_missing_initialize_errors() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);
    let insurance_id = env.register_contract(None, Insurance);
    let insurance_client = InsuranceClient::new(&env, &insurance_id);

    let result = insurance_client.try_create_policy(
        &user,
        &SorobanString::from_str(&env, "Test"),
        &CoverageType::Health,
        &500i128,
        &50_000i128,
        &None,
    );
    assert!(matches!(
        result,
        Err(Ok(InsuranceError::Unauthorized)) | Err(Ok(InsuranceError::NotInitialized))
    ));
}
