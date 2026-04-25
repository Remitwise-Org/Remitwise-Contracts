//! Multi-contract integration tests
//!
//! Validates cross-contract behaviour across:
//! - insurance
//! - bill_payments
//! - savings_goals
//! - remittance_split

use bill_payments::{BillPayments, BillPaymentsClient};
use insurance::{Insurance, InsuranceClient};
use remittance_split::{RemittanceSplit, RemittanceSplitClient};
use remitwise_common::CoverageType;
use savings_goals::{SavingsGoalContract, SavingsGoalContractClient};
use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String as SorobanString};

fn make_env() -> Env {
    let env = Env::default();
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 100,
        timestamp: 1_700_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 700_000,
    });
    env.mock_all_auths();
    env
}

#[test]
fn test_multi_contract_user_flow() {
    let env = make_env();
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
        &None,
        &SorobanString::from_str(&env, "XLM"),
    );
    assert_eq!(bill_id, 1u32, "Bill ID should be 1");

    let policy_id = insurance_client.create_policy(
        &user,
        &SorobanString::from_str(&env, "Health Insurance"),
        &CoverageType::Health,
        &200i128,
        &50_000i128,
        &None,
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
}

#[test]
fn test_split_with_rounding() {
    let env = make_env();
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
}

#[test]
fn test_multiple_entities_creation() {
    let env = make_env();
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

    let policy1 = insurance_client.create_policy(
        &user,
        &SorobanString::from_str(&env, "Life Insurance"),
        &CoverageType::Life,
        &150i128,
        &100_000i128,
        &None,
    );
    assert_eq!(policy1, 1u32);

    let policy2 = insurance_client.create_policy(
        &user,
        &SorobanString::from_str(&env, "Emergency Coverage"),
        &CoverageType::Health,
        &50i128,
        &10_000i128,
        &None,
    );
    assert_eq!(policy2, 2u32);
}
