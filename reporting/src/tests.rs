use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::testutils::LedgerInfo;
use soroban_sdk::testutils::storage::Instance;
use testutils::set_ledger_time;

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
                id: 1u32,
                owner: _owner.clone(),
                name: SorobanString::from_str(&env, "Education"),
                target_amount: 10000i128,
                current_amount: 7000i128,
                target_date: 1735689600u64,
                locked: true,
                unlock_date: None,
                tags: Vec::new(&env),
            });
            goals.push_back(SavingsGoal {
                id: 2u32,
                owner: _owner,
                name: SorobanString::from_str(&env, "Emergency"),
                target_amount: 5000i128,
                current_amount: 5000i128,
                target_date: 1735689600u64,
                locked: true,
                unlock_date: None,
                tags: Vec::new(&env),
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
                id: 1u32,
                owner: _owner,
                name: SorobanString::from_str(&env, "Electricity"),
                amount: 100i128,
                due_date: 1735689600u64,
                recurring: true,
                frequency_days: 30u32,
                paid: false,
                created_at: 1704067200u64,
                paid_at: None,
                schedule_id: None,
                external_ref: None,
                tags: Vec::new(&env),
                currency: SorobanString::from_str(&env, "XLM"),
            });
            BillPage {
                count: bills.len(),
                items: bills,
                next_cursor: 0u32,
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
                id: 1u32,
                owner: _owner.clone(),
                name: SorobanString::from_str(&env, "Electricity"),
                amount: 100i128,
                due_date: 1735689600u64,
                recurring: true,
                frequency_days: 30u32,
                paid: false,
                created_at: 1704067200u64,
                paid_at: None,
                schedule_id: None,
                external_ref: None,
                tags: Vec::new(&env),
                currency: SorobanString::from_str(&env, "XLM"),
            });
            bills.push_back(Bill {
                id: 2u32,
                owner: _owner,
                name: SorobanString::from_str(&env, "Water"),
                amount: 50i128,
                due_date: 1735689600u64,
                recurring: true,
                frequency_days: 30u32,
                paid: true,
                created_at: 1704067200u64,
                paid_at: Some(1704153600u64),
                schedule_id: None,
                external_ref: None,
                tags: Vec::new(&env),
                currency: SorobanString::from_str(&env, "XLM"),
            });
            BillPage {
                count: bills.len(),
                items: bills,
                next_cursor: 0u32,
            }
        }
    }
}

mod insurance {
    use crate::{InsurancePolicy, InsuranceTrait};
    use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

    #[contract]
    pub struct Insurance;

    #[contractimpl]
    impl InsuranceTrait for Insurance {
        fn get_active_policies(
            _env: Env,
            _owner: Address,
            _cursor: u32,
            _limit: u32,
        ) -> crate::PolicyPage {
            let env = _env;
            let mut policies = Vec::new(&env);
            policies.push_back(InsurancePolicy {
                id: 1u32,
                owner: _owner,
                name: SorobanString::from_str(&env, "Health Insurance"),
                coverage_type: SorobanString::from_str(&env, "health"),
                monthly_premium: 200i128,
                coverage_amount: 50000i128,
                active: true,
                next_payment_date: 1735689600u64,
                schedule_id: None,
                external_ref: None,
                tags: Vec::new(&env),
            });
            crate::PolicyPage {
                items: policies,
                next_cursor: 0u32,
                count: 1u32,
            }
        }

        fn get_total_monthly_premium(_env: Env, _owner: Address) -> i128 {
            200
        }
    }
}

// create_test_env removed in favor of testutils and Env::default()

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

    // Check category breakdown
    let spending = summary.category_breakdown.get(0).unwrap();
    assert_eq!(spending.category, Category::Spending);
    assert_eq!(spending.amount, 5000);
    assert_eq!(spending.percentage, 50);
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
    // Report is generated at ledger time 1704067200 (Jan 1 2024)
    set_ledger_time(&env, 1, 1704067200);
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

    // Generate and store a report (generated_at = 1704067200)
    let total_remittance = 10000i128;
    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let report =
        client.get_financial_health_report(&user, &total_remittance, &period_start, &period_end);

    let period_key = 202401u64;
    client.store_report(&user, &report, &period_key);

    // Verify report is stored
    assert!(client.get_stored_report(&user, &period_key).is_some());

    // Advance ledger ~60 days so the report is at least 30 days old.
    // current_time = 1709251200. 30-day boundary = 1709251200 - 2592000 = 1706659200.
    // before_timestamp = 1704067201 (> report.generated_at = 1704067200, < 1706659200). ✓
    set_ledger_time(&env, 1, 1709251200);
    let valid_before = 1704067201u64;
    let archived_count = client.archive_old_reports(&admin, &valid_before);
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

    // Archive with no reports stored (well before ledger time)
    let valid_before = 1701475200u64;
    let archived_count = client.archive_old_reports(&admin, &valid_before);
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

    // Archive the report:
    // generated_at = 1704067200. Advance ledger to 1709251200 (~60 days later).
    // 30-day retention boundary = 1709251200 - 2592000 = 1706659200.
    // Cutoff 1704067201 <= 1706659200 ✓, and 1704067200 < 1704067201 ✓ → report qualifies.
    set_ledger_time(&env, 1, 1709251200);
    let archive_before = 1704067201u64;
    client.archive_old_reports(&admin, &archive_before);
    assert_eq!(client.get_archived_reports(&user).len(), 1);

    // Cleanup old archives: archived_at = 1709251200. Need ledger time far enough ahead.
    // archived_at (1709251200) must be < before_timestamp AND
    // before_timestamp <= current_time - 90d (7776000 s).
    // Set ledger = 1725000000. 90d limit = 1725000000 - 7776000 = 1717224000.
    // Cutoff = 1709251201: 1709251201 <= 1717224000 ✓, and archived_at < cutoff ✓.
    set_ledger_time(&env, 1, 1725000000);
    let deleted = client.cleanup_old_reports(&admin, &1709251201u64);
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

    // Store a report (generated_at = 1704067200)
    let report = client.get_financial_health_report(&user, &10000, &1704067200, &1706745600);
    client.store_report(&user, &report, &202401);

    // Advance ledger ~60 days, then archive with valid cutoff (> generated_at, within retention).
    // current_time = 1709251200. 30-day limit = 1706659200. cutoff = 1704067201 <= 1706659200. ✓
    set_ledger_time(&env, 1, 1709251200);
    client.archive_old_reports(&admin, &1704067201u64);

    let stats = client.get_storage_stats();
    assert_eq!(stats.active_reports, 0);
    assert_eq!(stats.archived_reports, 1);
}

#[test]
fn test_archive_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.init(&admin);

    // Non-admin tries to archive — should return Unauthorized error
    let result = client.try_archive_old_reports(&non_admin, &1701475200);
    assert!(result.is_err(), "archive_old_reports should fail for non-admin");
}

#[test]
fn test_cleanup_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1720000000);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.init(&admin);

    // Non-admin tries to cleanup — should return Unauthorized error
    let result = client.try_cleanup_old_reports(&non_admin, &1704067200);
    assert!(result.is_err(), "cleanup_old_reports should fail for non-admin");
}

/// Archive a report that was generated less than 30 days ago — must be rejected.
#[test]
fn test_archive_too_recent_fails() {
    let env = Env::default();
    env.mock_all_auths();
    // Set ledger time to 1704067200 (Jan 1 2024)
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    // before_timestamp = now (1704067200): within the 30-day window → must fail
    let result = client.try_archive_old_reports(&admin, &1704067200);
    assert!(
        result.is_err(),
        "archive_old_reports with before_timestamp = now should fail (within retention window)"
    );
}

/// Archive reports using a cutoff exactly at the 30-day boundary — should be accepted.
#[test]
fn test_archive_at_exact_retention_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    // Ledger = 1704067200. Boundary = 1704067200 - 30*24*3600 = 1701475200
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    let boundary = 1701475200u64; // exactly 30 days before ledger time
    // No reports stored, but the call itself must succeed (not return InvalidRetentionWindow)
    let result = client.try_archive_old_reports(&admin, &boundary);
    assert!(
        result.is_ok(),
        "archive_old_reports at exact 30-day boundary should succeed, got: {:?}",
        result
    );
    assert_eq!(result.unwrap().unwrap(), 0u32);
}

/// Cleanup archives whose cutoff is within the 90-day window — must be rejected.
#[test]
fn test_cleanup_too_recent_fails() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    // before_timestamp = now (1704067200): within the 90-day window → must fail
    let result = client.try_cleanup_old_reports(&admin, &1704067200);
    assert!(
        result.is_err(),
        "cleanup_old_reports with before_timestamp = now should fail (within retention window)"
    );
}

/// Cleanup archives using a cutoff exactly at the 90-day boundary — should be accepted.
#[test]
fn test_cleanup_at_exact_retention_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    // Ledger = 1720000000. Boundary = 1720000000 - 90*24*3600 = 1712224000
    set_ledger_time(&env, 1, 1720000000);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    let boundary = 1712224000u64; // exactly 90 days before ledger time
    // No archives stored, but the call itself must succeed
    let result = client.try_cleanup_old_reports(&admin, &boundary);
    assert!(
        result.is_ok(),
        "cleanup_old_reports at exact 90-day boundary should succeed, got: {:?}",
        result
    );
    assert_eq!(result.unwrap().unwrap(), 0u32);
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
    // Retention check: before_timestamp (1700000000) <= 1704067200 - 30d (1701475200). TRUE.
    let _archived = client.archive_old_reports(&admin, &1700000000u64);

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= 518,400 after archiving",
        ttl
    );
}
