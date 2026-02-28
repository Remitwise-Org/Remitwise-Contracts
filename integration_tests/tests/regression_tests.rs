#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, String as SorobanString};

use bill_payments::{BillPayments, BillPaymentsClient};
use insurance::{Insurance, InsuranceClient};
use remittance_split::{RemittanceSplit, RemittanceSplitClient};

mod regression_tests {
    use super::*;

    #[test]
    #[should_panic(expected = "overflow")]
    fn bill_payments_total_unpaid_overflow_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let amount = i128::MAX / 2 + 1000;

        client.create_bill(
            &owner,
            &SorobanString::from_str(&env, "Bill1"),
            &amount,
            &1_000_000u64,
            &false,
            &0u32,
            &SorobanString::from_str(&env, "XLM"),
        );

        client.create_bill(
            &owner,
            &SorobanString::from_str(&env, "Bill2"),
            &amount,
            &1_000_000u64,
            &false,
            &0u32,
            &SorobanString::from_str(&env, "XLM"),
        );

        let _ = client.get_total_unpaid(&owner);
    }

    #[test]
    fn remittance_split_checked_arithmetic_overflow_returns_error() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.initialize_split(&owner, &0u64, &50u32, &30u32, &15u32, &5u32);

        let overflow_amount = i128::MAX / 50 + 1;
        let result = client.try_calculate_split(&overflow_amount);
        assert!(result.is_err());
    }

    #[test]
    fn insurance_non_owner_cannot_pay_premium() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let other = Address::generate(&env);

        let policy_id = client.create_policy(
            &owner,
            &SorobanString::from_str(&env, "Policy"),
            &SorobanString::from_str(&env, "health"),
            &100i128,
            &10_000i128,
        );

        let res = client.try_pay_premium(&other, &policy_id);
        assert!(res.is_err());
    }
}

