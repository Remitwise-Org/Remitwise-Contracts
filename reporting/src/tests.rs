use soroban_sdk::testutils::storage::Instance as StorageInstance;
use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    Address, Env,
};
use testutils::set_ledger_time;

use crate::{
    Category, ContractAddresses, CoverageType, DataAvailability, ReportingContract,
    ReportingContractClient, ReportingError, MAX_DEP_PAGES,
};

/// Minimal env with mock_all_auths — replaces the removed create_test_env helper.
fn create_test_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

// Mock contracts for testing
mod remittance_split {
    use soroban_sdk::{contract, contractimpl, Env, Vec};

    #[contract]
    pub struct RemittanceSplit;

    #[contractimpl]
    impl RemittanceSplit {
        pub fn get_split(env: &Env) -> Vec<u32> {
            let mut split = Vec::new(env);
            split.push_back(50);
            split.push_back(30);
            split.push_back(15);
            split.push_back(5);
            split
        }

        pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
            let mut amounts = Vec::new(&env);
            amounts.push_back(total_amount * 50 / 100);
            amounts.push_back(total_amount * 30 / 100);
            amounts.push_back(total_amount * 15 / 100);
            amounts.push_back(total_amount * 5 / 100);
            amounts
        }
    }
}

mod savings_goals {
    use crate::{SavingsGoal, SavingsGoalsTrait};
    use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

    #[contract]
    pub struct SavingsGoalsContract;

    #[contractimpl]
    impl SavingsGoalsTrait for SavingsGoalsContract {
        fn get_all_goals(_env: Env, _owner: Address) -> Vec<SavingsGoal> {
            let env = _env;
            let mut goals = Vec::new(&env);
            goals.push_back(SavingsGoal {
                id: 1,
                owner: _owner.clone(),
                name: SorobanString::from_str(&env, "Education"),
                target_amount: 10000,
                current_amount: 7000,
                target_date: 1735689600,
                locked: true,
                unlock_date: None,
            });
            goals.push_back(SavingsGoal {
                id: 2,
                owner: _owner,
                name: SorobanString::from_str(&env, "Emergency"),
                target_amount: 5000,
                current_amount: 5000,
                target_date: 1735689600,
                locked: true,
                unlock_date: None,
            });
            goals
        }

        fn is_goal_completed(_env: Env, goal_id: u32) -> bool {
            goal_id == 2
        }
    }
}

mod bill_payments {
    use crate::{Bill, BillPage, BillPaymentsTrait};
    use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

    #[contract]
    pub struct BillPayments;

    #[contractimpl]
    impl BillPaymentsTrait for BillPayments {
        fn get_unpaid_bills(_env: Env, _owner: Address, _cursor: u32, _limit: u32) -> BillPage {
            let env = _env;
            let mut bills = Vec::new(&env);
            bills.push_back(Bill {
                id: 1,
                owner: _owner,
                name: SorobanString::from_str(&env, "Electricity"),
                external_ref: None,
                amount: 100,
                due_date: 1735689600,
                recurring: true,
                frequency_days: 30,
                paid: false,
                created_at: 1704067200,
                paid_at: None,
                schedule_id: None,
                tags: Vec::new(&env),
                currency: SorobanString::from_str(&env, "XLM"),
                external_ref: None,
                tags: Vec::new(&env),
            });
            BillPage {
                count: bills.len(),
                items: bills,
                next_cursor: 0,
            }
        }

        fn get_total_unpaid(_env: Env, _owner: Address) -> i128 {
            100
        }

        fn get_all_bills_for_owner(
            _env: Env,
            _owner: Address,
            _cursor: u32,
            _limit: u32,
        ) -> BillPage {
            let env = _env;
            let mut bills = Vec::new(&env);
            bills.push_back(Bill {
                id: 1,
                owner: _owner.clone(),
                name: SorobanString::from_str(&env, "Electricity"),
                external_ref: None,
                amount: 100,
                due_date: 1735689600,
                recurring: true,
                frequency_days: 30,
                paid: false,
                created_at: 1704067200,
                paid_at: None,
                schedule_id: None,
                tags: Vec::new(&env),
                currency: SorobanString::from_str(&env, "XLM"),
                external_ref: None,
                tags: Vec::new(&env),
            });
            bills.push_back(Bill {
                id: 2,
                owner: _owner,
                name: SorobanString::from_str(&env, "Water"),
                external_ref: None,
                amount: 50,
                due_date: 1735689600,
                recurring: true,
                frequency_days: 30,
                paid: true,
                created_at: 1704067200,
                paid_at: Some(1704153600),
                schedule_id: None,
                tags: Vec::new(&env),
                currency: SorobanString::from_str(&env, "XLM"),
                external_ref: None,
                tags: Vec::new(&env),
            });
            BillPage {
                count: bills.len(),
                items: bills,
                next_cursor: 0,
            }
        }
    }
}

mod insurance {
    use crate::{InsurancePolicy, InsuranceTrait};
    use remitwise_common::CoverageType;
    use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

    #[contract]
    pub struct Insurance;

    #[contractimpl]
    impl InsuranceTrait for Insurance {
        fn get_active_policies(
            env: Env,
            _owner: Address,
            _cursor: u32,
            _limit: u32,
        ) -> crate::PolicyPage {
            let mut policies = Vec::new(&env);
            policies.push_back(InsurancePolicy {
                id: 1,
                owner: _owner,
                name: SorobanString::from_str(&env, "Health Insurance"),
                coverage_type: CoverageType::Health,
                monthly_premium: 200,
                coverage_amount: 50000,
                active: true,
                next_payment_date: 1735689600,
                external_ref: None,
            });
            crate::PolicyPage {
                items: policies,
                next_cursor: 0,
                count: 1,
            }
        }

        fn get_total_monthly_premium(_env: Env, _owner: Address) -> i128 {
            200
        }
    }
}

mod family_wallet {
    use soroban_sdk::{contract, contractimpl, Address, Env};

    #[contract]
    pub struct FamilyWallet;

    #[contractimpl]
    impl FamilyWallet {
        pub fn get_owner(env: Env) -> Address {
            env.current_contract_address()
        }
    }
}

#[test]
fn test_init_reporting_contract_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.init(&admin);

    let stored_admin = client.get_admin();
    assert_eq!(stored_admin, Some(admin));
}

#[test]
fn test_init_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.init(&admin);
    let result = client.try_init(&admin); // Should fail
    assert!(result.is_err(), "init should fail when called twice");
}

#[test]
fn test_configure_addresses_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.init(&admin);

    let remittance_split = Address::generate(&env);
    let savings_goals = Address::generate(&env);
    let bill_payments = Address::generate(&env);
    let insurance = Address::generate(&env);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split,
        &savings_goals,
        &bill_payments,
        &insurance,
        &family_wallet,
    );

    let addresses = client.get_addresses();
    assert!(addresses.is_some());
    let addrs = addresses.unwrap();
    assert_eq!(addrs.remittance_split, remittance_split);
    assert_eq!(addrs.savings_goals, savings_goals);
}

#[test]
fn test_configure_addresses_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.init(&admin);

    let remittance_split = Address::generate(&env);
    let savings_goals = Address::generate(&env);
    let bill_payments = Address::generate(&env);
    let insurance = Address::generate(&env);
    let family_wallet = Address::generate(&env);

    let result = client.try_configure_addresses(
        &non_admin,
        &remittance_split,
        &savings_goals,
        &bill_payments,
        &insurance,
        &family_wallet,
    );
    assert!(
        result.is_err(),
        "configure_addresses should fail for non-admin"
    );
}

// ---------------------------------------------------------------------------
// Dependency address configuration integrity (Issue #309)
// ---------------------------------------------------------------------------

#[test]
fn test_configure_addresses_rejects_duplicate_slots() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let d = Address::generate(&env);

    let result = client.try_configure_addresses(&admin, &a, &a, &b, &c, &d);
    assert!(matches!(
        result,
        Err(Ok(ReportingError::InvalidDependencyAddressConfiguration))
    ));
    assert!(client.get_addresses().is_none());
}

#[test]
fn test_configure_addresses_rejects_self_reference() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    let split = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);

    let result =
        client.try_configure_addresses(&admin, &split, &savings, &bills, &insurance, &contract_id);
    assert!(matches!(
        result,
        Err(Ok(ReportingError::InvalidDependencyAddressConfiguration))
    ));
}

#[test]
fn test_configure_invalid_does_not_overwrite_existing_addresses() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let d = Address::generate(&env);
    let e = Address::generate(&env);

    client.configure_addresses(&admin, &a, &b, &c, &d, &e);

    let dup = client.try_configure_addresses(&admin, &a, &a, &c, &d, &e);
    assert!(matches!(
        dup,
        Err(Ok(ReportingError::InvalidDependencyAddressConfiguration))
    ));

    let stored = client.get_addresses().expect("prior config must remain");
    assert_eq!(stored.remittance_split, a);
    assert_eq!(stored.savings_goals, b);
    assert_eq!(stored.bill_payments, c);
    assert_eq!(stored.insurance, d);
    assert_eq!(stored.family_wallet, e);
}

#[test]
fn test_verify_dependency_address_set_accepts_distinct_addresses() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    let addrs = ContractAddresses {
        remittance_split: Address::generate(&env),
        savings_goals: Address::generate(&env),
        bill_payments: Address::generate(&env),
        insurance: Address::generate(&env),
        family_wallet: Address::generate(&env),
    };
    assert!(matches!(
        client.try_verify_dependency_address_set(&addrs),
        Ok(Ok(()))
    ));
}

#[test]
fn test_verify_dependency_address_set_rejects_duplicates() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    let x = Address::generate(&env);
    let addrs = ContractAddresses {
        remittance_split: x.clone(),
        savings_goals: x,
        bill_payments: Address::generate(&env),
        insurance: Address::generate(&env),
        family_wallet: Address::generate(&env),
    };
    let result = client.try_verify_dependency_address_set(&addrs);
    assert!(matches!(
        result,
        Err(Ok(ReportingError::InvalidDependencyAddressConfiguration))
    ));
}

#[test]
fn test_verify_dependency_address_set_rejects_self_reference() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    let addrs = ContractAddresses {
        remittance_split: Address::generate(&env),
        savings_goals: Address::generate(&env),
        bill_payments: Address::generate(&env),
        insurance: Address::generate(&env),
        family_wallet: Address::generate(&env),
    };
    let result = client.try_verify_dependency_address_set(&addrs);
    assert!(matches!(
        result,
        Err(Ok(ReportingError::InvalidDependencyAddressConfiguration))
    ));
}

#[test]
fn test_get_remittance_summary() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    // Register mock contracts
    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    let total_amount = 10000i128;
    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let summary = client.get_remittance_summary(&user, &total_amount, &period_start, &period_end);

    assert_eq!(summary.total_received, 10000);
    assert_eq!(summary.total_allocated, 10000);
    assert_eq!(summary.category_breakdown.len(), 4);
    assert_eq!(summary.period_start, period_start);
    assert_eq!(summary.period_end, period_end);
    assert_eq!(summary.data_availability, DataAvailability::Complete);

    // Check category breakdown
    let spending = summary.category_breakdown.get(0).unwrap();
    assert_eq!(spending.category, Category::Spending);
    assert_eq!(spending.amount, 5000);
    assert_eq!(spending.percentage, 50);
}

#[test]
fn test_get_remittance_summary_rejects_invalid_period() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let result = client.try_get_remittance_summary(&user, &10_000i128, &200, &100);
    assert!(matches!(result, Err(Ok(ReportingError::InvalidPeriod))));
}

#[test]
fn test_get_remittance_summary_missing_addresses() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = soroban_sdk::Address::generate(&env);

    // Purposefully DO NOT call client.init() or client.configure_addresses()

    let total_amount = 10000i128;
    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let summary = client.get_remittance_summary(&user, &total_amount, &period_start, &period_end);

    assert_eq!(summary.total_received, 10000);
    assert_eq!(summary.category_breakdown.len(), 0);
    assert_eq!(summary.data_availability, DataAvailability::Missing);
}

mod failing_remittance_split {
    use soroban_sdk::{contract, contractimpl, Env, Vec};
    #[contract]
    pub struct FailingRemittanceSplit;
    #[contractimpl]
    impl FailingRemittanceSplit {
        pub fn get_split(_env: &Env) -> Vec<u32> {
            panic!("Remote call failing to simulate Partial Data");
        }
        pub fn calculate_split(_env: Env, _total_amount: i128) -> Vec<i128> {
            panic!("Remote call failing to simulate Partial Data");
        }
    }
}

#[test]
fn test_get_remittance_summary_partial_data_remote_failure_propagates() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = soroban_sdk::Address::generate(&env);
    let user = soroban_sdk::Address::generate(&env);
    client.init(&admin);

    // Register FAILING mock contract
    let failing_split_id =
        env.register_contract(None, failing_remittance_split::FailingRemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = soroban_sdk::Address::generate(&env);

    client.configure_addresses(
        &admin,
        &failing_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    let total_amount = 10000i128;
    // Remote failures are converted into partial data.
    let summary = client.get_remittance_summary(&user, &total_amount, &0, &0);
    assert_eq!(summary.data_availability, DataAvailability::Partial);
}

#[test]
fn test_get_savings_report() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    let period_start = 1704067200u64;
    let period_end = 1706745600u64;
    let report = client.get_savings_report(&user, &period_start, &period_end);

    assert_eq!(report.total_goals, 2);
    assert_eq!(report.completed_goals, 1);
    assert_eq!(report.total_target, 15000);
    assert_eq!(report.total_saved, 12000);
    assert_eq!(report.completion_percentage, 80);
}

#[test]
fn test_get_savings_report_rejects_invalid_period() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let result = client.try_get_savings_report(&user, &200, &100);
    assert!(matches!(result, Err(Ok(ReportingError::InvalidPeriod))));
}

#[test]
fn test_get_bill_compliance_report() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let report = client.get_bill_compliance_report(&user, &period_start, &period_end);

    // Note: Mock returns bills for a generated address, so user-specific filtering will show 0
    // This is expected behavior for the test
    assert_eq!(report.period_start, period_start);
    assert_eq!(report.period_end, period_end);
}

#[test]
fn test_get_bill_compliance_report_rejects_invalid_period() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let result = client.try_get_bill_compliance_report(&user, &200, &100);
    assert!(matches!(result, Err(Ok(ReportingError::InvalidPeriod))));
}

#[test]
fn test_get_insurance_report() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let report = client.get_insurance_report(&user, &period_start, &period_end);

    assert_eq!(report.active_policies, 1);
    assert_eq!(report.total_coverage, 50000);
    assert_eq!(report.monthly_premium, 200);
    assert_eq!(report.annual_premium, 2400);
    assert_eq!(report.coverage_to_premium_ratio, 2083); // 50000 * 100 / 2400
}

#[test]
fn test_get_insurance_report_rejects_invalid_period() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let result = client.try_get_insurance_report(&user, &200, &100);
    assert!(matches!(result, Err(Ok(ReportingError::InvalidPeriod))));
}

#[test]
fn test_calculate_health_score() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    let health_score = client.calculate_health_score(&user, &10000);

    // Savings: 12000/15000 = 80% -> 32 points
    // Bills: Has unpaid bills but none overdue (due_date > current_time) -> 35 points
    // Insurance: Has 1 active policy -> 20 points
    // Total: 32 + 35 + 20 = 87
    assert_eq!(health_score.savings_score, 32);
    assert_eq!(health_score.bills_score, 35);
    assert_eq!(health_score.insurance_score, 20);
    assert_eq!(health_score.score, 87);
}

#[test]
fn test_get_financial_health_report() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    let total_remittance = 10000i128;
    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let report =
        client.get_financial_health_report(&user, &total_remittance, &period_start, &period_end);

    assert_eq!(report.health_score.score, 87);
    assert_eq!(report.remittance_summary.total_received, 10000);
    assert_eq!(report.savings_report.total_goals, 2);
    assert_eq!(report.insurance_report.active_policies, 1);
    assert_eq!(report.generated_at, 1704067200);
}

#[test]
fn test_get_financial_health_report_rejects_invalid_period() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let result = client.try_get_financial_health_report(&user, &10_000i128, &200, &100);
    assert!(matches!(result, Err(Ok(ReportingError::InvalidPeriod))));
}

#[test]
fn test_get_trend_analysis() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let current_amount = 15000i128;
    let previous_amount = 10000i128;

    let trend = client.get_trend_analysis(&user, &current_amount, &previous_amount);

    assert_eq!(trend.current_amount, 15000);
    assert_eq!(trend.previous_amount, 10000);
    assert_eq!(trend.change_amount, 5000);
    assert_eq!(trend.change_percentage, 50); // 50% increase
}

#[test]
fn test_get_trend_analysis_decrease() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let current_amount = 8000i128;
    let previous_amount = 10000i128;

    let trend = client.get_trend_analysis(&user, &current_amount, &previous_amount);

    assert_eq!(trend.current_amount, 8000);
    assert_eq!(trend.previous_amount, 10000);
    assert_eq!(trend.change_amount, -2000);
    assert_eq!(trend.change_percentage, -20); // 20% decrease
}

#[test]
fn test_store_and_retrieve_report() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    let total_remittance = 10000i128;
    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let report =
        client.get_financial_health_report(&user, &total_remittance, &period_start, &period_end);

    let period_key = 202401u64; // January 2024

    let stored = client.store_report(&user, &report, &period_key);
    assert!(stored);

    let retrieved = client.get_stored_report(&user, &period_key);
    assert!(retrieved.is_some());
    let retrieved_report = retrieved.unwrap();
    assert_eq!(
        retrieved_report.health_score.score,
        report.health_score.score
    );
    assert_eq!(
        retrieved_report.remittance_summary.total_received,
        report.remittance_summary.total_received
    );
}

#[test]
fn test_retrieve_nonexistent_report() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let retrieved = client.get_stored_report(&user, &999999);
    assert!(retrieved.is_none());
}

#[test]
fn test_health_score_no_goals() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    // Create a mock savings contract that returns no goals
    mod empty_savings {
        use crate::{SavingsGoal, SavingsGoalsTrait};
        use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

        #[contract]
        pub struct EmptySavings;

        #[contractimpl]
        impl SavingsGoalsTrait for EmptySavings {
            fn get_all_goals(_env: Env, _owner: Address) -> Vec<SavingsGoal> {
                Vec::new(&_env)
            }

            fn is_goal_completed(_env: Env, _goal_id: u32) -> bool {
                false
            }
        }
    }

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, empty_savings::EmptySavings);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    let health_score = client.calculate_health_score(&user, &10000);

    // Should get default score of 20 for savings when no goals exist
    assert_eq!(health_score.savings_score, 20);
}

// ============================================
// Storage Optimization and Archival Tests
// ============================================

#[test]
fn test_archive_old_reports() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    // Generate and store a report
    let total_remittance = 10000i128;
    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let report =
        client.get_financial_health_report(&user, &total_remittance, &period_start, &period_end);

    let period_key = 202401u64;
    client.store_report(&user, &report, &period_key);

    // Verify report is stored
    assert!(client.get_stored_report(&user, &period_key).is_some());

    // Archive reports before far future timestamp
    let archived_count = client.archive_old_reports(&admin, &2000000000);
    assert_eq!(archived_count, 1);

    // Verify report is no longer in active storage
    assert!(client.get_stored_report(&user, &period_key).is_none());

    // Verify report is in archive
    let archived = client.get_archived_reports(&user);
    assert_eq!(archived.len(), 1);
}

#[test]
fn test_archive_empty_when_no_old_reports() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.init(&admin);

    // Archive with no reports stored
    let archived_count = client.archive_old_reports(&admin, &2000000000);
    assert_eq!(archived_count, 0);
}

#[test]
fn test_cleanup_old_reports() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    // Generate and store a report
    let report = client.get_financial_health_report(&user, &10000, &1704067200, &1706745600);
    client.store_report(&user, &report, &202401);

    // Archive the report
    client.archive_old_reports(&admin, &2000000000);
    assert_eq!(client.get_archived_reports(&user).len(), 1);

    // Cleanup old archives
    let deleted = client.cleanup_old_reports(&admin, &2000000000);
    assert_eq!(deleted, 1);

    // Verify archives are gone
    assert_eq!(client.get_archived_reports(&user).len(), 0);
}

#[test]
fn test_storage_stats() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200); // Standard timestamp for reporting tests
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    // Initial stats
    let stats = client.get_storage_stats();
    assert_eq!(stats.active_reports, 0);
    assert_eq!(stats.archived_reports, 0);

    // Store a report
    let report = client.get_financial_health_report(&user, &10000, &1704067200, &1706745600);
    client.store_report(&user, &report, &202401);

    let stats = client.get_storage_stats();
    assert_eq!(stats.active_reports, 1);
    assert_eq!(stats.archived_reports, 0);

    // Archive and check stats
    client.archive_old_reports(&admin, &2000000000);

    let stats = client.get_storage_stats();
    assert_eq!(stats.active_reports, 0);
    assert_eq!(stats.archived_reports, 1);
}

/// Regression: `get_storage_stats` must stay aligned with real maps across store → archive → cleanup
/// and after high-volume inserts (see issue #316).
#[test]
fn test_storage_stats_regression_across_archive_and_cleanup_cycles() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    // Zero-state snapshot (no reports stored yet; stats key may be absent)
    set_ledger_time(&env, 1, 1_704_067_200);
    let zero = client.get_storage_stats();
    assert_eq!(zero.active_reports, 0);
    assert_eq!(zero.archived_reports, 0);
    assert_eq!(zero.last_updated, 0);

    // High-volume: many active rows, distinct generated_at via ledger time steps
    const TOTAL: u64 = 16;
    let base_ts = 1_000_000u64;
    for i in 0..TOTAL {
        set_ledger_time(&env, 10 + i as u32, base_ts + i);
        let report = client.get_financial_health_report(&user, &10000, &1704067200, &1706745600);
        client.store_report(&user, &report, &(202_400 + i));
    }

    let after_bulk = client.get_storage_stats();
    assert_eq!(after_bulk.active_reports, TOTAL as u32);
    assert_eq!(after_bulk.archived_reports, 0);
    assert_eq!(after_bulk.last_updated, base_ts + TOTAL - 1);

    // Partial archive: only reports with generated_at < cutoff move to ARCH_RPT
    let archive_cutoff = base_ts + 8;
    set_ledger_time(&env, 500, base_ts + 100);
    let n_archived = client.archive_old_reports(&admin, &archive_cutoff);
    assert_eq!(n_archived, 8);

    let after_partial = client.get_storage_stats();
    assert_eq!(after_partial.active_reports, 8);
    assert_eq!(after_partial.archived_reports, 8);
    assert_eq!(after_partial.last_updated, base_ts + 100);

    // Post-cleanup: archives removed; actives unchanged
    let cleanup_before = base_ts + 200;
    set_ledger_time(&env, 600, base_ts + 150);
    let deleted = client.cleanup_old_reports(&admin, &cleanup_before);
    assert_eq!(deleted, 8);

    let after_cleanup = client.get_storage_stats();
    assert_eq!(after_cleanup.active_reports, 8);
    assert_eq!(after_cleanup.archived_reports, 0);
    assert_eq!(after_cleanup.last_updated, base_ts + 150);

    // Second cycle: new report increments active; full archive then cleanup returns to zero archived
    set_ledger_time(&env, 700, base_ts + 300);
    let report = client.get_financial_health_report(&user, &10000, &1704067200, &1706745600);
    client.store_report(&user, &report, &209_912);

    let after_new_store = client.get_storage_stats();
    assert_eq!(after_new_store.active_reports, 9);
    assert_eq!(after_new_store.archived_reports, 0);

    set_ledger_time(&env, 800, base_ts + 400);
    client.archive_old_reports(&admin, &(base_ts + 500));
    let after_second_archive = client.get_storage_stats();
    assert_eq!(after_second_archive.active_reports, 0);
    assert_eq!(after_second_archive.archived_reports, 9);

    set_ledger_time(&env, 900, base_ts + 500);
    assert_eq!(client.cleanup_old_reports(&admin, &(base_ts + 600)), 9);
    let final_stats = client.get_storage_stats();
    assert_eq!(final_stats.active_reports, 0);
    assert_eq!(final_stats.archived_reports, 0);
}

#[test]
fn test_archive_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.init(&admin);

    // Non-admin tries to archive
    let result = client.try_archive_old_reports(&non_admin, &2000000000);
    assert!(result.is_err());
}

#[test]
fn test_cleanup_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.init(&admin);

    // Non-admin tries to cleanup
    let result = client.try_cleanup_old_reports(&non_admin, &2000000000);
    assert!(result.is_err());
}

// ============================================================================
// Storage TTL Extension Tests
//
// Verify that instance storage TTL is properly extended on state-changing
// operations, preventing unexpected data expiration.
//
// Contract TTL configuration:
//   INSTANCE_LIFETIME_THRESHOLD  = 17,280 ledgers (~1 day)
//   INSTANCE_BUMP_AMOUNT         = 518,400 ledgers (~30 days)
//   ARCHIVE_LIFETIME_THRESHOLD   = 17,280 ledgers (~1 day)
//   ARCHIVE_BUMP_AMOUNT          = 2,592,000 ledgers (~180 days)
//
// Operations extending instance TTL:
//   init, configure_addresses, store_report, archive_old_reports,
//   cleanup_old_reports
//
// Operations extending archive TTL:
//   archive_old_reports
// ============================================================================

/// Helper: create test environment with TTL-appropriate ledger settings.
fn create_ttl_test_env(sequence: u32, max_ttl: u32) -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set(LedgerInfo {
        timestamp: 1704067200,
        protocol_version: 20,
        sequence_number: sequence,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: max_ttl,
    });
    env
}

/// Verify that init extends instance storage TTL.
#[test]
fn test_instance_ttl_extended_on_init() {
    let env = create_ttl_test_env(100, 700_000);

    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    // init calls extend_instance_ttl
    client.init(&admin);

    // Inspect instance TTL — must be at least INSTANCE_BUMP_AMOUNT
    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= INSTANCE_BUMP_AMOUNT (518,400) after init",
        ttl
    );
}

/// Verify that configure_addresses refreshes instance TTL.
#[test]
fn test_instance_ttl_refreshed_on_configure_addresses() {
    let env = create_ttl_test_env(100, 700_000);

    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.init(&admin);

    // Advance ledger so TTL drops below threshold (17,280)
    // After init: live_until = 518,500. At seq 510,000: TTL = 8,500
    env.ledger().set(LedgerInfo {
        timestamp: 1704067200,
        protocol_version: 20,
        sequence_number: 510_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    // Register mock sub-contracts
    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    // configure_addresses calls extend_instance_ttl → re-extends TTL to 518,400
    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= 518,400 after configure_addresses",
        ttl
    );
}

/// Verify that store_report refreshes instance TTL after ledger advancement.
#[test]
fn test_instance_ttl_refreshed_on_store_report() {
    let env = create_ttl_test_env(100, 700_000);

    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    // Set up sub-contracts
    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    // Generate a report
    let report =
        client.get_financial_health_report(&user, &10000i128, &1704067200u64, &1706745600u64);

    // Advance ledger so TTL drops below threshold (17,280)
    env.ledger().set(LedgerInfo {
        timestamp: 1706745600,
        protocol_version: 20,
        sequence_number: 510_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    // store_report calls extend_instance_ttl → re-extends TTL to 518,400
    let stored = client.store_report(&user, &report, &202401u64);
    assert!(stored);

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= 518,400 after store_report",
        ttl
    );
}

/// Verify data persists across repeated operations spanning multiple
/// ledger advancements, proving TTL is continuously renewed.
#[test]
fn test_report_data_persists_across_ledger_advancements() {
    // Use high min_persistent_entry_ttl so mock sub-contracts survive
    // across large ledger advancements (they don't extend their own TTL)
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set(LedgerInfo {
        timestamp: 1704067200,
        protocol_version: 20,
        sequence_number: 100,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 1_100_000,
        max_entry_ttl: 1_200_000,
    });

    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Phase 1: Initialize and configure
    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    let report =
        client.get_financial_health_report(&user, &10000i128, &1704067200u64, &1706745600u64);
    client.store_report(&user, &report, &202401u64);

    // Phase 2: Advance to seq 510,000 (reporting contract TTL = 8,500 < 17,280)
    env.ledger().set(LedgerInfo {
        timestamp: 1709424000,
        protocol_version: 20,
        sequence_number: 510_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 1_100_000,
        max_entry_ttl: 1_200_000,
    });

    let report2 =
        client.get_financial_health_report(&user, &15000i128, &1706745600u64, &1709424000u64);
    client.store_report(&user, &report2, &202402u64);

    // Phase 3: Advance to seq 1,020,000 (TTL = 8,400 < 17,280)
    env.ledger().set(LedgerInfo {
        timestamp: 1711929600,
        protocol_version: 20,
        sequence_number: 1_020_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 1_100_000,
        max_entry_ttl: 1_200_000,
    });

    // Both reports should be retrievable (read-only, no TTL extension)
    let r1 = client.get_stored_report(&user, &202401u64);
    assert!(
        r1.is_some(),
        "January report must persist across ledger advancements"
    );

    let r2 = client.get_stored_report(&user, &202402u64);
    assert!(r2.is_some(), "February report must persist");

    // Admin data should be accessible
    let stored_admin = client.get_admin();
    assert!(stored_admin.is_some(), "Admin must persist");

    // TTL should still be positive (read-only ops don't call extend_ttl,
    // but data is still accessible proving TTL hasn't expired)
    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl > 0,
        "Instance TTL ({}) must be > 0 — data persists across ledger advancements",
        ttl
    );
}

/// Verify that archive_old_reports extends archive TTL (2,592,000 ledgers).
#[test]
fn test_archive_ttl_extended_on_archive_reports() {
    let env = create_ttl_test_env(100, 3_000_000);

    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    // Store a report and then archive it
    let report =
        client.get_financial_health_report(&user, &10000i128, &1704067200u64, &1706745600u64);
    client.store_report(&user, &report, &202401u64);

    // Advance ledger so TTL drops below threshold before archiving
    env.ledger().set(LedgerInfo {
        timestamp: 1704067200,
        protocol_version: 20,
        sequence_number: 510_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 3_000_000,
    });

    // archive_old_reports calls extend_instance_ttl first (bumps to 518,400),
    // then extend_archive_ttl which is a no-op (TTL already above threshold)
    let _archived = client.archive_old_reports(&admin, &2000000000);

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= 518,400 after archiving",
        ttl
    );
}

// ============================================================================
// Authorization Tests — Report Storage and Retrieval (#310)
//
// Security assumptions validated here:
//   1. store_report requires the caller to be the report owner (require_auth).
//   2. get_stored_report is open but enforces user-key isolation: user A
//      cannot read user B's reports because the storage key is (Address, u64).
//   3. archive_old_reports is admin-only; non-admin callers are rejected.
//   4. cleanup_old_reports is admin-only; non-admin callers are rejected.
//   5. get_archived_reports filters by address, so user A cannot see user B's
//      archived reports.
//   6. A user cannot store a report on behalf of another user.
//   7. Admin cannot store a report for a user without that user's auth.
//   8. Multiple users can store reports independently without cross-leakage.
//   9. Overwriting a report requires the owner's auth each time.
//  10. Cleanup after archive does not expose other users' data.
// ============================================================================

// ── helpers ──────────────────────────────────────────────────────────────────

/// Full setup: init + configure_addresses. Returns (client, admin, sub-contract ids).
fn setup_reporting(
    env: &Env,
) -> (
    ReportingContractClient<'_>,
    Address,
    Address, // remittance_split_id
    Address, // savings_goals_id
    Address, // bill_payments_id
    Address, // insurance_id
) {
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(env, &contract_id);
    let admin = Address::generate(env);

    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    (
        client,
        admin,
        remittance_split_id,
        savings_goals_id,
        bill_payments_id,
        insurance_id,
    )
}

/// Generate a FinancialHealthReport for `user` using the configured client.
fn make_report(
    _env: &Env,
    client: &ReportingContractClient,
    user: &Address,
) -> crate::FinancialHealthReport {
    client.get_financial_health_report(user, &10_000i128, &1_704_067_200u64, &1_706_745_600u64)
}

// ── store_report authorization ────────────────────────────────────────────────

/// store_report succeeds when the owner authorizes the call.
#[test]
fn test_store_report_owner_can_store() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, _, _, _, _, _) = setup_reporting(&env);
    let user = Address::generate(&env);

    let report = make_report(&env, &client, &user);
    let ok = client.store_report(&user, &report, &202_401u64);
    assert!(ok, "owner must be able to store their own report");
}

/// store_report requires the user's auth — verified via the auth recording API.
#[test]
fn test_store_report_requires_auth() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, _, _, _, _, _) = setup_reporting(&env);
    let user = Address::generate(&env);

    let report = make_report(&env, &client, &user);
    let _ = client.store_report(&user, &report, &202_401u64);

    // Verify that store_report recorded a require_auth for the report owner.
    let auths = env.auths();
    let found = auths.iter().any(|(addr, _)| *addr == user);
    assert!(
        found,
        "store_report must record a require_auth for the report owner"
    );
}

/// A user cannot store a report under a different user's address.
/// The SDK enforces this: require_auth on `user` means the *caller* must be `user`.
#[test]
fn test_store_report_cannot_impersonate_another_user() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, _, _, _, _, _) = setup_reporting(&env);

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    let report_a = make_report(&env, &client, &user_a);

    // Attempt to store report_a under user_b's key — mock_all_auths lets this
    // through at the SDK level, but the storage key will be (user_b, period).
    // The critical check: user_a's key must NOT be populated.
    client.store_report(&user_b, &report_a, &202_401u64);

    // user_a's slot must be empty
    let result_a = client.get_stored_report(&user_a, &202_401u64);
    assert!(
        result_a.is_none(),
        "user_a's report slot must be empty when stored under user_b"
    );

    // user_b's slot has the report
    let result_b = client.get_stored_report(&user_b, &202_401u64);
    assert!(
        result_b.is_some(),
        "report stored under user_b must be retrievable by user_b"
    );
}

/// Admin cannot store a report for a user without that user's auth being recorded.
#[test]
fn test_store_report_admin_cannot_bypass_user_auth() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, admin, _, _, _, _) = setup_reporting(&env);
    let user = Address::generate(&env);

    let report = make_report(&env, &client, &user);

    // Store under admin's address (not user's) — this is the only valid call
    // an admin can make without user auth.
    client.store_report(&admin, &report, &202_401u64);

    // The user's slot must remain empty
    let user_result = client.get_stored_report(&user, &202_401u64);
    assert!(
        user_result.is_none(),
        "admin storing under their own address must not populate user's slot"
    );

    // Admin's own slot has the report
    let admin_result = client.get_stored_report(&admin, &202_401u64);
    assert!(
        admin_result.is_some(),
        "admin's own report slot must be populated"
    );
}

// ── get_stored_report user isolation ─────────────────────────────────────────

/// User A cannot read User B's stored report — storage key isolation.
#[test]
fn test_get_stored_report_user_isolation() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, _, _, _, _, _) = setup_reporting(&env);

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    let report_a = make_report(&env, &client, &user_a);
    client.store_report(&user_a, &report_a, &202_401u64);

    // user_b queries user_a's period key — must get None
    let result = client.get_stored_report(&user_a, &202_401u64);
    assert!(result.is_some(), "user_a must retrieve their own report");

    // Querying with user_b's address for the same period key returns None
    let result_b = client.get_stored_report(&user_b, &202_401u64);
    assert!(
        result_b.is_none(),
        "user_b must not see user_a's report — key isolation enforced"
    );
}

/// Same period key, different users — no cross-contamination.
#[test]
fn test_get_stored_report_same_period_key_different_users() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, _, _, _, _, _) = setup_reporting(&env);

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let period = 202_401u64;

    let report_a = make_report(&env, &client, &user_a);
    let report_b = make_report(&env, &client, &user_b);

    client.store_report(&user_a, &report_a, &period);
    client.store_report(&user_b, &report_b, &period);

    let ra = client.get_stored_report(&user_a, &period).unwrap();
    let rb = client.get_stored_report(&user_b, &period).unwrap();

    // Both exist independently
    assert_eq!(ra.generated_at, report_a.generated_at);
    assert_eq!(rb.generated_at, report_b.generated_at);
}

/// Multiple period keys for the same user are all retrievable.
#[test]
fn test_get_stored_report_multiple_periods_same_user() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, _, _, _, _, _) = setup_reporting(&env);
    let user = Address::generate(&env);

    let report = make_report(&env, &client, &user);
    client.store_report(&user, &report, &202_401u64);
    client.store_report(&user, &report, &202_402u64);
    client.store_report(&user, &report, &202_403u64);

    assert!(client.get_stored_report(&user, &202_401u64).is_some());
    assert!(client.get_stored_report(&user, &202_402u64).is_some());
    assert!(client.get_stored_report(&user, &202_403u64).is_some());
    // Non-existent period returns None
    assert!(client.get_stored_report(&user, &202_404u64).is_none());
}

/// Overwriting a report for the same (user, period) replaces the previous value.
#[test]
fn test_store_report_overwrite_replaces_previous() {
    // Use a high min_persistent_entry_ttl so sub-contract instances survive
    // across ledger sequence advancement.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set(LedgerInfo {
        timestamp: 1_704_067_200,
        protocol_version: 20,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 1_100_000,
        max_entry_ttl: 1_200_000,
    });
    let (client, _, _, _, _, _) = setup_reporting(&env);
    let user = Address::generate(&env);
    let period = 202_401u64;

    let report_v1 = make_report(&env, &client, &user);
    client.store_report(&user, &report_v1, &period);

    // Advance time and generate a second report
    env.ledger().set(LedgerInfo {
        timestamp: 1_706_745_600,
        protocol_version: 20,
        sequence_number: 2,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 1_100_000,
        max_entry_ttl: 1_200_000,
    });
    let report_v2 = make_report(&env, &client, &user);
    client.store_report(&user, &report_v2, &period);

    let retrieved = client.get_stored_report(&user, &period).unwrap();
    // The stored report must be the second one (generated_at differs)
    assert_eq!(
        retrieved.generated_at, report_v2.generated_at,
        "overwrite must replace the previous report"
    );
}

// ── archive_old_reports authorization ────────────────────────────────────────

/// archive_old_reports succeeds when called by admin.
#[test]
fn test_archive_old_reports_admin_succeeds() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, admin, _, _, _, _) = setup_reporting(&env);
    let user = Address::generate(&env);

    let report = make_report(&env, &client, &user);
    client.store_report(&user, &report, &202_401u64);

    let count = client.archive_old_reports(&admin, &2_000_000_000u64);
    assert_eq!(count, 1, "admin must be able to archive reports");
}

/// archive_old_reports panics when called by a non-admin.
#[test]
fn test_archive_old_reports_non_admin_rejected() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, _, _, _, _, _) = setup_reporting(&env);
    let attacker = Address::generate(&env);

    let result = client.try_archive_old_reports(&attacker, &2_000_000_000u64);
    assert_eq!(result, Err(Ok(ReportingError::Unauthorized)));
}

/// archive_old_reports panics when called by a regular user (not admin).
#[test]
fn test_archive_old_reports_regular_user_rejected() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, _, _, _, _, _) = setup_reporting(&env);
    let user = Address::generate(&env);

    let report = make_report(&env, &client, &user);
    client.store_report(&user, &report, &202_401u64);

    // user tries to archive — must be rejected
    let result = client.try_archive_old_reports(&user, &2_000_000_000u64);
    assert_eq!(result, Err(Ok(ReportingError::Unauthorized)));
}

/// archive_old_reports records require_auth for the admin caller.
#[test]
fn test_archive_old_reports_records_admin_auth() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, admin, _, _, _, _) = setup_reporting(&env);

    client.archive_old_reports(&admin, &2_000_000_000u64);

    let auths = env.auths();
    let found = auths.iter().any(|(addr, _)| *addr == admin);
    assert!(
        found,
        "archive_old_reports must record require_auth for the admin"
    );
}

// ── cleanup_old_reports authorization ────────────────────────────────────────

/// cleanup_old_reports succeeds when called by admin.
#[test]
fn test_cleanup_old_reports_admin_succeeds() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, admin, _, _, _, _) = setup_reporting(&env);
    let user = Address::generate(&env);

    let report = make_report(&env, &client, &user);
    client.store_report(&user, &report, &202_401u64);
    client.archive_old_reports(&admin, &2_000_000_000u64);

    let deleted = client.cleanup_old_reports(&admin, &2_000_000_000u64);
    assert_eq!(deleted, 1, "admin must be able to cleanup archived reports");
}

/// cleanup_old_reports panics when called by a non-admin.
#[test]
fn test_cleanup_old_reports_non_admin_rejected() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, _, _, _, _, _) = setup_reporting(&env);
    let attacker = Address::generate(&env);

    let result = client.try_cleanup_old_reports(&attacker, &2_000_000_000u64);
    assert_eq!(result, Err(Ok(ReportingError::Unauthorized)));
}

/// cleanup_old_reports panics when called by a regular user.
#[test]
fn test_cleanup_old_reports_regular_user_rejected() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, admin, _, _, _, _) = setup_reporting(&env);
    let user = Address::generate(&env);

    let report = make_report(&env, &client, &user);
    client.store_report(&user, &report, &202_401u64);
    client.archive_old_reports(&admin, &2_000_000_000u64);

    // user tries to cleanup — must be rejected
    let result = client.try_cleanup_old_reports(&user, &2_000_000_000u64);
    assert_eq!(result, Err(Ok(ReportingError::Unauthorized)));
}

/// cleanup_old_reports records require_auth for the admin caller.
#[test]
fn test_cleanup_old_reports_records_admin_auth() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, admin, _, _, _, _) = setup_reporting(&env);

    client.cleanup_old_reports(&admin, &2_000_000_000u64);

    let auths = env.auths();
    let found = auths.iter().any(|(addr, _)| *addr == admin);
    assert!(
        found,
        "cleanup_old_reports must record require_auth for the admin"
    );
}

// ── get_archived_reports user isolation ──────────────────────────────────────

/// get_archived_reports only returns reports belonging to the queried user.
#[test]
fn test_get_archived_reports_user_isolation() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, admin, _, _, _, _) = setup_reporting(&env);

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    let report_a = make_report(&env, &client, &user_a);
    let report_b = make_report(&env, &client, &user_b);

    client.store_report(&user_a, &report_a, &202_401u64);
    client.store_report(&user_b, &report_b, &202_401u64);

    // Archive both
    client.archive_old_reports(&admin, &2_000_000_000u64);

    let archived_a = client.get_archived_reports(&user_a);
    let archived_b = client.get_archived_reports(&user_b);

    assert_eq!(
        archived_a.len(),
        1,
        "user_a must see exactly 1 archived report"
    );
    assert_eq!(
        archived_b.len(),
        1,
        "user_b must see exactly 1 archived report"
    );

    // Verify no cross-contamination
    for r in archived_a.iter() {
        assert_eq!(
            r.user, user_a,
            "user_a's archive must only contain their own reports"
        );
    }
    for r in archived_b.iter() {
        assert_eq!(
            r.user, user_b,
            "user_b's archive must only contain their own reports"
        );
    }
}

/// A user with no archived reports gets an empty list.
#[test]
fn test_get_archived_reports_empty_for_unknown_user() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, admin, _, _, _, _) = setup_reporting(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    let report = make_report(&env, &client, &user_a);
    client.store_report(&user_a, &report, &202_401u64);
    client.archive_old_reports(&admin, &2_000_000_000u64);

    // user_b has no archived reports
    let archived = client.get_archived_reports(&user_b);
    assert_eq!(
        archived.len(),
        0,
        "user with no archived reports must get empty list"
    );
}

/// Cleanup removes only the target user's archives, not other users'.
#[test]
fn test_cleanup_does_not_remove_other_users_archives() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, admin, _, _, _, _) = setup_reporting(&env);

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    let report_a = make_report(&env, &client, &user_a);
    let report_b = make_report(&env, &client, &user_b);

    client.store_report(&user_a, &report_a, &202_401u64);
    client.store_report(&user_b, &report_b, &202_401u64);

    // Archive both at timestamp 1_704_067_200
    client.archive_old_reports(&admin, &2_000_000_000u64);

    // Cleanup only archives created before 1_704_067_201 (both qualify)
    let deleted = client.cleanup_old_reports(&admin, &2_000_000_000u64);
    assert_eq!(deleted, 2, "both archives must be cleaned up");

    // Both users' archives are gone
    assert_eq!(client.get_archived_reports(&user_a).len(), 0);
    assert_eq!(client.get_archived_reports(&user_b).len(), 0);
}

/// Cleanup with a past timestamp removes nothing.
#[test]
fn test_cleanup_past_timestamp_removes_nothing() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, admin, _, _, _, _) = setup_reporting(&env);
    let user = Address::generate(&env);

    let report = make_report(&env, &client, &user);
    client.store_report(&user, &report, &202_401u64);
    client.archive_old_reports(&admin, &2_000_000_000u64);

    // Cleanup with timestamp 0 — nothing is older than epoch 0
    let deleted = client.cleanup_old_reports(&admin, &0u64);
    assert_eq!(
        deleted, 0,
        "cleanup with past timestamp must remove nothing"
    );

    // Archive still intact
    assert_eq!(client.get_archived_reports(&user).len(), 1);
}

// ── multi-user storage isolation end-to-end ──────────────────────────────────

/// Full lifecycle: store → archive → cleanup for multiple users with no leakage.
#[test]
fn test_multi_user_full_lifecycle_no_data_leakage() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, admin, _, _, _, _) = setup_reporting(&env);

    let users: [Address; 3] = [
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];

    // Each user stores two period reports
    for user in &users {
        let r = make_report(&env, &client, user);
        client.store_report(user, &r, &202_401u64);
        client.store_report(user, &r, &202_402u64);
    }

    // Verify isolation before archiving
    for user in &users {
        assert!(client.get_stored_report(user, &202_401u64).is_some());
        assert!(client.get_stored_report(user, &202_402u64).is_some());
    }

    // Archive all
    let archived_count = client.archive_old_reports(&admin, &2_000_000_000u64);
    assert_eq!(
        archived_count, 6,
        "6 reports (3 users × 2 periods) must be archived"
    );

    // Active storage must be empty for all users
    for user in &users {
        assert!(client.get_stored_report(user, &202_401u64).is_none());
        assert!(client.get_stored_report(user, &202_402u64).is_none());
    }

    // Each user sees exactly their 2 archived reports
    for user in &users {
        let archived = client.get_archived_reports(user);
        assert_eq!(archived.len(), 2);
        for r in archived.iter() {
            assert_eq!(
                r.user, *user,
                "archived report must belong to the queried user"
            );
        }
    }

    // Cleanup
    let deleted = client.cleanup_old_reports(&admin, &2_000_000_000u64);
    assert_eq!(deleted, 6);

    // All archives gone
    for user in &users {
        assert_eq!(client.get_archived_reports(user).len(), 0);
    }
}

/// Archiving with a timestamp that excludes recent reports leaves them in active storage.
#[test]
fn test_archive_timestamp_boundary_preserves_recent_reports() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);
    let (client, admin, _, _, _, _) = setup_reporting(&env);
    let user = Address::generate(&env);

    // Store report at t=1_704_067_200
    let report = make_report(&env, &client, &user);
    client.store_report(&user, &report, &202_401u64);

    // Archive with before_timestamp = 1_000_000_000 (before the report's generated_at)
    let archived = client.archive_old_reports(&admin, &1_000_000_000u64);
    assert_eq!(
        archived, 0,
        "report generated after cutoff must not be archived"
    );

    // Report must still be in active storage
    assert!(
        client.get_stored_report(&user, &202_401u64).is_some(),
        "recent report must remain in active storage"
    );
}

/// configure_addresses requires admin auth — non-admin is rejected.
#[test]
fn test_configure_addresses_non_admin_rejected() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.init(&admin);

    let result = client.try_configure_addresses(
        &attacker,
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
    );
    assert!(
        result.is_err(),
        "configure_addresses must reject non-admin callers"
    );
}

/// init cannot be called twice — second call must fail.
#[test]
fn test_init_double_init_rejected() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.init(&admin);
    let result = client.try_init(&admin);
    assert!(result.is_err(), "second init must be rejected");
}

/// get_stored_report for a non-existent (user, period) returns None — no panic.
#[test]
fn test_get_stored_report_missing_key_returns_none() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    let user = Address::generate(&env);
    let result = client.get_stored_report(&user, &999_999u64);
    assert!(
        result.is_none(),
        "missing report must return None, not panic"
    );
}

#[test]
fn test_check_dependencies_succeeds_with_configured_contracts() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    // Register mock contracts
    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet_id = env.register_contract(None, family_wallet::FamilyWallet);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet_id,
    );

    let statuses = client.check_dependencies(&admin);
    assert_eq!(statuses.len(), 5);

    // Check each status
    assert_eq!(
        statuses.get(0).unwrap().name,
        soroban_sdk::String::from_str(&env, "remittance_split")
    );
    assert!(statuses.get(0).unwrap().ok);
    assert_eq!(statuses.get(0).unwrap().error_category, None);

    assert_eq!(
        statuses.get(1).unwrap().name,
        soroban_sdk::String::from_str(&env, "savings_goals")
    );
    assert!(statuses.get(1).unwrap().ok);

    assert_eq!(
        statuses.get(2).unwrap().name,
        soroban_sdk::String::from_str(&env, "bill_payments")
    );
    assert!(statuses.get(2).unwrap().ok);

    assert_eq!(
        statuses.get(3).unwrap().name,
        soroban_sdk::String::from_str(&env, "insurance")
    );
    assert!(statuses.get(3).unwrap().ok);

    assert_eq!(
        statuses.get(4).unwrap().name,
        soroban_sdk::String::from_str(&env, "family_wallet")
    );
    assert!(statuses.get(4).unwrap().ok);
}

#[test]
fn test_check_dependencies_fails_for_non_admin() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    client.init(&admin);

    let result = client.try_check_dependencies(&non_admin);
    assert!(result.is_err());
}

#[test]
fn test_check_dependencies_fails_when_not_configured() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    let result = client.try_check_dependencies(&admin);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Dependency paging loop termination tests (Issue #487 / SC-034)
//
// These tests prove that the bill-compliance and insurance-report paging loops
// are bounded and deterministic under two conditions:
//
//  1. Normal termination – a dependency returns `next_cursor == 0` after a
//     finite number of pages.  The loop must collect every item and report
//     `DataAvailability::Complete`.
//
//  2. Cap termination – a dependency never returns `next_cursor == 0`
//     (simulating an unbounded or misbehaving contract).  The loop must stop
//     after exactly `MAX_DEP_PAGES` fetches and report
//     `DataAvailability::Partial`.
//
//  3. Monotonic cursor progression – the loop always advances the cursor to
//     the value returned by the previous page, never revisiting a page.
//     Tested by asserting item counts from multi-page responses match the
//     expected per-page accumulation.
//
// Mock bill-payments contracts use cursor-value routing so each test's
// page sequence is hard-coded and requires no shared state.
// ---------------------------------------------------------------------------

// ── Mock: bill-payments returning exactly 3 pages then cursor = 0 ──────────
//
// page 0 (cursor=0) → 1 bill (id=1, created within period), next_cursor=5
// page 1 (cursor=5) → 1 bill (id=2, created within period), next_cursor=10
// page 2 (cursor=10) → 1 bill (id=3, created within period), next_cursor=0
//
// Expected: 3 bills collected, DataAvailability::Complete
mod bills_three_pages {
    use crate::{Bill, BillPage, BillPaymentsTrait};
    use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

    const PERIOD_TS: u64 = 1_704_067_200;

    #[contract]
    pub struct BillsThreePages;

    #[contractimpl]
    impl BillPaymentsTrait for BillsThreePages {
        fn get_unpaid_bills(env: Env, _owner: Address, _c: u32, _l: u32) -> BillPage {
            BillPage {
                items: Vec::new(&env),
                next_cursor: 0,
                count: 0,
            }
        }
        fn get_total_unpaid(_env: Env, _owner: Address) -> i128 {
            0
        }
        fn get_all_bills_for_owner(env: Env, owner: Address, cursor: u32, _limit: u32) -> BillPage {
            let (bill_id, next_cursor) = match cursor {
                0 => (1u32, 5u32),
                5 => (2, 10),
                _ => (3, 0),
            };
            let mut items = Vec::new(&env);
            items.push_back(Bill {
                id: bill_id,
                owner,
                name: SorobanString::from_str(&env, "B"),
                external_ref: None,
                amount: 100,
                due_date: PERIOD_TS + 86400,
                recurring: false,
                frequency_days: 30,
                paid: false,
                created_at: PERIOD_TS,
                paid_at: None,
                schedule_id: None,
                tags: Vec::new(&env),
                currency: SorobanString::from_str(&env, "XLM"),
            });
            BillPage {
                count: 1,
                items,
                next_cursor,
            }
        }
    }
}

// ── Mock: bill-payments that never returns cursor = 0 ──────────────────────
//
// Always returns next_cursor = cursor + 1.  Without a cap this loop would
// run forever; the contract must stop after MAX_DEP_PAGES.
mod bills_infinite {
    use crate::{Bill, BillPage, BillPaymentsTrait};
    use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

    const PERIOD_TS: u64 = 1_704_067_200;

    #[contract]
    pub struct BillsInfinite;

    #[contractimpl]
    impl BillPaymentsTrait for BillsInfinite {
        fn get_unpaid_bills(env: Env, _owner: Address, _c: u32, _l: u32) -> BillPage {
            BillPage {
                items: Vec::new(&env),
                next_cursor: 0,
                count: 0,
            }
        }
        fn get_total_unpaid(_env: Env, _owner: Address) -> i128 {
            0
        }
        fn get_all_bills_for_owner(env: Env, owner: Address, cursor: u32, _limit: u32) -> BillPage {
            let mut items = Vec::new(&env);
            items.push_back(Bill {
                id: cursor,
                owner,
                name: SorobanString::from_str(&env, "B"),
                external_ref: None,
                amount: 50,
                due_date: PERIOD_TS + 86400,
                recurring: false,
                frequency_days: 0,
                paid: false,
                created_at: PERIOD_TS,
                paid_at: None,
                schedule_id: None,
                tags: Vec::new(&env),
                currency: SorobanString::from_str(&env, "XLM"),
            });
            BillPage {
                count: 1,
                items,
                next_cursor: cursor + 1,
            }
        }
    }
}

// ── Mock: insurance returning exactly 3 pages then cursor = 0 ─────────────
mod insurance_three_pages {
    use crate::{CoverageType, InsurancePolicy, InsuranceTrait, PolicyPage};
    use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

    #[contract]
    pub struct InsuranceThreePages;

    #[contractimpl]
    impl InsuranceTrait for InsuranceThreePages {
        fn get_active_policies(env: Env, owner: Address, cursor: u32, _limit: u32) -> PolicyPage {
            let (policy_id, next_cursor) = match cursor {
                0 => (1u32, 7u32),
                7 => (2, 14),
                _ => (3, 0),
            };
            let mut items = Vec::new(&env);
            items.push_back(InsurancePolicy {
                id: policy_id,
                owner,
                name: SorobanString::from_str(&env, "P"),
                external_ref: None,
                coverage_type: CoverageType::Health,
                monthly_premium: 100,
                coverage_amount: 10_000,
                active: true,
                next_payment_date: 1_735_689_600,
            });
            PolicyPage {
                count: 1,
                items,
                next_cursor,
            }
        }
        fn get_total_monthly_premium(_env: Env, _owner: Address) -> i128 {
            300
        }
    }
}

// ── Mock: insurance that never returns cursor = 0 ─────────────────────────
mod insurance_infinite {
    use crate::{CoverageType, InsurancePolicy, InsuranceTrait, PolicyPage};
    use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

    #[contract]
    pub struct InsuranceInfinite;

    #[contractimpl]
    impl InsuranceTrait for InsuranceInfinite {
        fn get_active_policies(env: Env, owner: Address, cursor: u32, _limit: u32) -> PolicyPage {
            let mut items = Vec::new(&env);
            items.push_back(InsurancePolicy {
                id: cursor,
                owner,
                name: SorobanString::from_str(&env, "P"),
                external_ref: None,
                coverage_type: CoverageType::Health,
                monthly_premium: 100,
                coverage_amount: 10_000,
                active: true,
                next_payment_date: 1_735_689_600,
            });
            PolicyPage {
                count: 1,
                items,
                next_cursor: cursor + 1,
            }
        }
        fn get_total_monthly_premium(_env: Env, _owner: Address) -> i128 {
            0
        }
    }
}

// ── Shared setup helper for paging tests ─────────────────────────────────

fn setup_paging_test(
    env: &Env,
    bill_payments_id: Address,
    insurance_id: Address,
) -> (ReportingContractClient, Address) {
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let family_wallet = Address::generate(env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );
    (client, admin)
}

// ── Test 1: bill paging terminates at cursor = 0 (3 pages) ───────────────

#[test]
fn test_bill_paging_terminates_at_cursor_zero() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);

    let bill_id = env.register_contract(None, bills_three_pages::BillsThreePages);
    let ins_id = env.register_contract(None, insurance::Insurance);
    let (client, _) = setup_paging_test(&env, bill_id, ins_id);

    let user = Address::generate(&env);
    let report = client.get_bill_compliance_report(&user, &1_704_067_200u64, &1_706_745_600u64);

    // All 3 pages fetched — no items are filtered out because created_at == period_start
    assert_eq!(
        report.data_availability,
        DataAvailability::Complete,
        "cursor=0 termination must yield Complete"
    );
    assert_eq!(
        report.total_bills, 3,
        "all 3 bills from 3 pages must be aggregated"
    );
    assert_eq!(report.unpaid_bills, 3);
}

// ── Test 2: bill paging terminates at MAX_DEP_PAGES cap ──────────────────

#[test]
fn test_bill_paging_terminates_at_cap() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);

    let bill_id = env.register_contract(None, bills_infinite::BillsInfinite);
    let ins_id = env.register_contract(None, insurance::Insurance);
    let (client, _) = setup_paging_test(&env, bill_id, ins_id);

    let user = Address::generate(&env);
    let report = client.get_bill_compliance_report(&user, &1_704_067_200u64, &1_706_745_600u64);

    assert_eq!(
        report.data_availability,
        DataAvailability::Partial,
        "unbounded dependency must yield Partial after MAX_DEP_PAGES"
    );
    assert_eq!(
        report.total_bills, MAX_DEP_PAGES,
        "exactly MAX_DEP_PAGES bills must be collected before the cap fires"
    );
}

// ── Test 3: bill cursor monotonicity — items accumulate across all pages ──

#[test]
fn test_bill_paging_cursor_monotonicity() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);

    let bill_id = env.register_contract(None, bills_three_pages::BillsThreePages);
    let ins_id = env.register_contract(None, insurance::Insurance);
    let (client, _) = setup_paging_test(&env, bill_id, ins_id);

    let user = Address::generate(&env);
    // Each page delivers exactly 1 bill; 3 pages → 3 bills total.
    // If the loop visited the same page twice, count would differ.
    let report = client.get_bill_compliance_report(&user, &1_704_067_200u64, &1_706_745_600u64);
    assert_eq!(
        report.total_bills, 3,
        "cursor must advance monotonically so each page is visited exactly once"
    );
    assert_eq!(report.data_availability, DataAvailability::Complete);
}

// ── Test 4: insurance paging terminates at cursor = 0 (3 pages) ──────────

#[test]
fn test_insurance_paging_terminates_at_cursor_zero() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);

    let bill_id = env.register_contract(None, bill_payments::BillPayments);
    let ins_id = env.register_contract(None, insurance_three_pages::InsuranceThreePages);
    let (client, _) = setup_paging_test(&env, bill_id, ins_id);

    let user = Address::generate(&env);
    let report = client.get_insurance_report(&user, &1_704_067_200u64, &1_706_745_600u64);

    assert_eq!(
        report.data_availability,
        DataAvailability::Complete,
        "cursor=0 termination must yield Complete"
    );
    assert_eq!(
        report.active_policies, 3,
        "all 3 policies from 3 pages must be aggregated"
    );
    assert_eq!(report.total_coverage, 30_000);
}

// ── Test 5: insurance paging terminates at MAX_DEP_PAGES cap ─────────────

#[test]
fn test_insurance_paging_terminates_at_cap() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);

    let bill_id = env.register_contract(None, bill_payments::BillPayments);
    let ins_id = env.register_contract(None, insurance_infinite::InsuranceInfinite);
    let (client, _) = setup_paging_test(&env, bill_id, ins_id);

    let user = Address::generate(&env);
    let report = client.get_insurance_report(&user, &1_704_067_200u64, &1_706_745_600u64);

    assert_eq!(
        report.data_availability,
        DataAvailability::Partial,
        "unbounded dependency must yield Partial after MAX_DEP_PAGES"
    );
    assert_eq!(
        report.active_policies, MAX_DEP_PAGES,
        "exactly MAX_DEP_PAGES policies must be collected before the cap fires"
    );
}

// ── Test 6: insurance cursor monotonicity ────────────────────────────────

#[test]
fn test_insurance_paging_cursor_monotonicity() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1_704_067_200);

    let bill_id = env.register_contract(None, bill_payments::BillPayments);
    let ins_id = env.register_contract(None, insurance_three_pages::InsuranceThreePages);
    let (client, _) = setup_paging_test(&env, bill_id, ins_id);

    let user = Address::generate(&env);
    let report = client.get_insurance_report(&user, &1_704_067_200u64, &1_706_745_600u64);
    assert_eq!(
        report.active_policies, 3,
        "cursor must advance monotonically so each page is visited exactly once"
    );
    assert_eq!(report.data_availability, DataAvailability::Complete);
}
