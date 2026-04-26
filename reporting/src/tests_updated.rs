use soroban_sdk::xdr::LedgerInfo;
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
                amount: 100,
                due_date: 1735689600,
                recurring: true,
                frequency_days: 30,
                paid: false,
                created_at: 1704067200,
                paid_at: None,
                schedule_id: None,
                currency: SorobanString::from_str(&env, "XLM"),
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
                amount: 100,
                due_date: 1735689600,
                recurring: true,
                frequency_days: 30,
                paid: false,
                created_at: 1704067200,
                paid_at: None,
                schedule_id: None,
                currency: SorobanString::from_str(&env, "XLM"),
            });
            bills.push_back(Bill {
                id: 2,
                owner: _owner,
                name: SorobanString::from_str(&env, "Water"),
                amount: 50,
                due_date: 1735689600,
                recurring: true,
                frequency_days: 30,
                paid: true,
                created_at: 1704067200,
                paid_at: Some(1704153600),
                schedule_id: None,
                currency: SorobanString::from_str(&env, "XLM"),
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
    use crate::{InsurancePolicy, InsuranceTrait, PolicyPage};
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
                id: 1,
                owner: _owner,
                name: SorobanString::from_str(&env, "Health Insurance"),
                coverage_type: SorobanString::from_str(&env, "health"),
                monthly_premium: 200,
                coverage_amount: 50000,
                active: true,
                next_payment_date: 1735689600,
                schedule_id: None,
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

/// Helper function to create test environment (non-auth version)
fn create_test_env() -> soroban_sdk::Env {
    let env = soroban_sdk::Env::default();
    set_ledger_time(&env, 1, 1704067200);
    env
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
    let env = create_test_env();
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

// ============================================================================
// ADDRESS GUARD TESTS - Verify all endpoints return proper errors when ADDRS unset
// ============================================================================

#[test]
fn test_get_remittance_summary_addresses_not_configured() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Init but don't configure addresses
    client.init(&admin);

    let total_amount = 10000i128;
    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let result = client.try_get_remittance_summary(&user, &total_amount, &period_start, &period_end);
    assert!(result.is_err(), "Should fail when addresses not configured");
}

#[test]
fn test_get_savings_report_addresses_not_configured() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let result = client.try_get_savings_report(&user, &period_start, &period_end);
    assert!(result.is_err(), "Should fail when addresses not configured");
}

#[test]
fn test_get_bill_compliance_report_addresses_not_configured() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let result = client.try_get_bill_compliance_report(&user, &period_start, &period_end);
    assert!(result.is_err(), "Should fail when addresses not configured");
}

#[test]
fn test_get_insurance_report_addresses_not_configured() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let result = client.try_get_insurance_report(&user, &period_start, &period_end);
    assert!(result.is_err(), "Should fail when addresses not configured");
}

#[test]
fn test_calculate_health_score_addresses_not_configured() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let result = client.try_calculate_health_score(&user, &10000);
    assert!(result.is_err(), "Should fail when addresses not configured");
}

#[test]
fn test_get_financial_health_report_addresses_not_configured() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    let total_remittance = 10000i128;
    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let result = client.try_get_financial_health_report(&user, &total_remittance, &period_start, &period_end);
    assert!(result.is_err(), "Should fail when addresses not configured");
}

// ============================================================================
// SUCCESSFUL REPORT GENERATION TESTS - With addresses configured
// ============================================================================

#[test]
fn test_get_remittance_summary() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200);
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

    let result = client.try_get_remittance_summary(&user, &total_amount, &period_start, &period_end);
    assert!(result.is_ok(), "Should succeed when addresses configured");
    let summary = result.unwrap();

    assert_eq!(summary.total_received, 10000);
    assert_eq!(summary.total_allocated, 10000);
    assert_eq!(summary.category_breakdown.len(), 4);
    assert_eq!(summary.period_start, period_start);
    assert_eq!(summary.period_end, period_end);

    // Check category breakdown
    let spending = summary.category_breakdown.get(0).unwrap();
    assert_eq!(spending.amount, 5000);
    assert_eq!(spending.percentage, 50);
}

#[test]
fn test_get_savings_report() {
    let env = Env::default();
    env.mock_all_auths();
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

    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let result = client.try_get_savings_report(&user, &period_start, &period_end);
    assert!(result.is_ok(), "Should succeed when addresses configured");
    let report = result.unwrap();

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

    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let result = client.try_get_bill_compliance_report(&user, &period_start, &period_end);
    assert!(result.is_ok(), "Should succeed when addresses configured");
    let report = result.unwrap();

    assert_eq!(report.period_start, period_start);
    assert_eq!(report.period_end, period_end);
}

#[test]
fn test_get_insurance_report() {
    let env = Env::default();
    env.mock_all_auths();
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

    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let result = client.try_get_insurance_report(&user, &period_start, &period_end);
    assert!(result.is_ok(), "Should succeed when addresses configured");
    let report = result.unwrap();

    assert_eq!(report.active_policies, 1);
    assert_eq!(report.total_coverage, 50000);
    assert_eq!(report.monthly_premium, 200);
    assert_eq!(report.annual_premium, 2400);
    assert_eq!(report.coverage_to_premium_ratio, 2083);
}

#[test]
fn test_calculate_health_score() {
    let env = Env::default();
    env.mock_all_auths();
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

    let result = client.try_calculate_health_score(&user, &10000);
    assert!(result.is_ok(), "Should succeed when addresses configured");
    let health_score = result.unwrap();

    assert_eq!(health_score.savings_score, 32);
    assert_eq!(health_score.bills_score, 35);
    assert_eq!(health_score.insurance_score, 20);
    assert_eq!(health_score.score, 87);
}

#[test]
fn test_get_financial_health_report() {
    let env = Env::default();
    env.mock_all_auths();
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

    let total_remittance = 10000i128;
    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let result = client.try_get_financial_health_report(&user, &total_remittance, &period_start, &period_end);
    assert!(result.is_ok(), "Should succeed when addresses configured");
    let report = result.unwrap();

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
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let current_amount = 15000i128;
    let previous_amount = 10000i128;

    let trend = client.get_trend_analysis(&user, &current_amount, &previous_amount);

    assert_eq!(trend.current_amount, 15000);
    assert_eq!(trend.previous_amount, 10000);
    assert_eq!(trend.change_amount, 5000);
    assert_eq!(trend.change_percentage, 50);
}

#[test]
fn test_get_trend_analysis_decrease() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let current_amount = 8000i128;
    let previous_amount = 10000i128;

    let trend = client.get_trend_analysis(&user, &current_amount, &previous_amount);

    assert_eq!(trend.current_amount, 8000);
    assert_eq!(trend.previous_amount, 10000);
    assert_eq!(trend.change_amount, -2000);
    assert_eq!(trend.change_percentage, -20);
}

#[test]
fn test_store_and_retrieve_report() {
    let env = Env::default();
    env.mock_all_auths();
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

    let total_remittance = 10000i128;
    let period_start = 1704067200u64;
    let period_end = 1706745600u64;

    let result = client.try_get_financial_health_report(&user, &total_remittance, &period_start, &period_end);
    assert!(result.is_ok());
    let report = result.unwrap();

    let period_key = 202401u64;

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
    set_ledger_time(&env, 1, 1704067200);
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
    set_ledger_time(&env, 1, 1704067200);
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

    let result = client.try_calculate_health_score(&user, &10000);
    assert!(result.is_ok());
    let health_score = result.unwrap();

    // Should get default score of 20 for savings when no goals exist
    assert_eq!(health_score.savings_score, 20);
}

// ============================================================================
// ADMIN OPERATION TESTS - Archive and cleanup with proper error handling
// ============================================================================

#[test]
fn test_archive_old_reports() {
    let env = Env::default();
    env.mock_all_auths();
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

    let result = client.try_get_financial_health_report(&user, &10000i128, &1704067200u64, &1706745600u64);
    assert!(result.is_ok());
    let report = result.unwrap();

    let period_key = 202401u64;
    client.store_report(&user, &report, &period_key);

    assert!(client.get_stored_report(&user, &period_key).is_some());

    let archive_result = client.try_archive_old_reports(&admin, &2000000000);
    assert!(archive_result.is_ok(), "Archive should succeed");
    assert_eq!(archive_result.unwrap(), 1);

    assert!(client.get_stored_report(&user, &period_key).is_none());

    let archived = client.get_archived_reports(&user);
    assert_eq!(archived.len(), 1);
}

#[test]
fn test_archive_empty_when_no_old_reports() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.init(&admin);

    let archive_result = client.try_archive_old_reports(&admin, &2000000000);
    assert!(archive_result.is_ok(), "Archive should succeed with empty reports");
    assert_eq!(archive_result.unwrap(), 0);
}

#[test]
fn test_cleanup_old_reports() {
    let env = Env::default();
    env.mock_all_auths();
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

    let result = client.try_get_financial_health_report(&user, &10000i128, &1704067200u64, &1706745600u64);
    assert!(result.is_ok());
    let report = result.unwrap();
    client.store_report(&user, &report, &202401);

    let archive_result = client.try_archive_old_reports(&admin, &2000000000);
    assert!(archive_result.is_ok());

    assert_eq!(client.get_archived_reports(&user).len(), 1);

    let cleanup_result = client.try_cleanup_old_reports(&admin, &2000000000);
    assert!(cleanup_result.is_ok(), "Cleanup should succeed");
    assert_eq!(cleanup_result.unwrap(), 1);

    assert_eq!(client.get_archived_reports(&user).len(), 0);
}

#[test]
fn test_archive_unauthorized() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.init(&admin);

    let result = client.try_archive_old_reports(&non_admin, &2000000000);
    assert!(result.is_err(), "Non-admin should not be able to archive");
}

#[test]
fn test_cleanup_unauthorized() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.init(&admin);

    let result = client.try_cleanup_old_reports(&non_admin, &2000000000);
    assert!(result.is_err(), "Non-admin should not be able to cleanup");
}

#[test]
fn test_storage_stats() {
    let env = Env::default();
    env.mock_all_auths();
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

    let stats = client.get_storage_stats();
    assert_eq!(stats.active_reports, 0);
    assert_eq!(stats.archived_reports, 0);

    let result = client.try_get_financial_health_report(&user, &10000i128, &1704067200u64, &1706745600u64);
    assert!(result.is_ok());
    let report = result.unwrap();
    client.store_report(&user, &report, &202401);

    let archive_result = client.try_archive_old_reports(&admin, &2000000000);
    assert!(archive_result.is_ok());

    let stats = client.get_storage_stats();
    assert_eq!(stats.active_reports, 0);
    assert_eq!(stats.archived_reports, 1);
}

// ============================================================================
// Storage TTL Extension Tests - Verify TTL is properly extended
// ============================================================================

fn create_ttl_test_env(sequence: u32, max_ttl: u32) -> soroban_sdk::Env {
    let env = soroban_sdk::Env::default();
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

#[test]
fn test_instance_ttl_extended_on_init() {
    let env = create_ttl_test_env(100, 700_000);

    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.init(&admin);

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= INSTANCE_BUMP_AMOUNT (518,400) after init",
        ttl
    );
}

#[test]
fn test_instance_ttl_refreshed_on_configure_addresses() {
    let env = create_ttl_test_env(100, 700_000);

    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.init(&admin);

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

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= 518,400 after configure_addresses",
        ttl
    );
}

#[test]
fn test_instance_ttl_refreshed_on_store_report() {
    let env = create_ttl_test_env(100, 700_000);

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

    let result = client.try_get_financial_health_report(&user, &10000i128, &1704067200u64, &1706745600u64);
    assert!(result.is_ok());
    let report = result.unwrap();

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

    let stored = client.store_report(&user, &report, &202401u64);
    assert!(stored);

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= 518,400 after store_report",
        ttl
    );
}

#[test]
fn test_report_data_persists_across_ledger_advancements() {
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

    let result1 = client.try_get_financial_health_report(&user, &10000i128, &1704067200u64, &1706745600u64);
    assert!(result1.is_ok());
    let report = result1.unwrap();
    client.store_report(&user, &report, &202401u64);

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

    let result2 = client.try_get_financial_health_report(&user, &15000i128, &1706745600u64, &1709424000u64);
    assert!(result2.is_ok());
    let report2 = result2.unwrap();
    client.store_report(&user, &report2, &202402u64);

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

    let r1 = client.get_stored_report(&user, &202401u64);
    assert!(
        r1.is_some(),
        "January report must persist across ledger advancements"
    );

    let r2 = client.get_stored_report(&user, &202402u64);
    assert!(r2.is_some(), "February report must persist");

    let stored_admin = client.get_admin();
    assert!(stored_admin.is_some(), "Admin must persist");

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl > 0,
        "Instance TTL ({}) must be > 0 — data persists across ledger advancements",
        ttl
    );
}

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

    let result = client.try_get_financial_health_report(&user, &10000i128, &1704067200u64, &1706745600u64);
    assert!(result.is_ok());
    let report = result.unwrap();
    client.store_report(&user, &report, &202401u64);

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

    let archive_result = client.try_archive_old_reports(&admin, &2000000000);
    assert!(archive_result.is_ok());

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= 518,400 after archiving",
        ttl
    );
}
