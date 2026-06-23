use soroban_sdk::testutils::Address as _;
use soroban_sdk::{
    contract, contractimpl, symbol_short, Address, Env, String as SorobanString, Vec,
};
use testutils::set_ledger_time;

use crate::{
    Bill, BillPage, BillPaymentsTrait, Category, CoverageType, DataAvailability, GoalPage,
    InsurancePolicy, InsuranceTrait, PolicyPage, RemittanceSplitTrait, ReportingContract,
    ReportingContractClient, SavingsGoal, SavingsGoalsTrait, MAX_DEP_PAGES,
};

const PERIOD_START: u64 = 1_704_067_200;
const PERIOD_END: u64 = 1_706_745_600;
const MODE_FULL: u32 = 0;
const MODE_EMPTY: u32 = 1;
const MODE_OVER_LIMIT: u32 = 2;
const MODE_FAILING: u32 = 3;

fn create_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, PERIOD_START);
    env
}

fn bill(env: &Env, owner: Address, id: u32, paid: bool) -> Bill {
    Bill {
        id,
        owner,
        name: SorobanString::from_str(env, "Bill"),
        external_ref: None,
        amount: 100,
        due_date: PERIOD_END,
        recurring: false,
        frequency_days: 0,
        paid,
        created_at: PERIOD_START,
        paid_at: if paid { Some(PERIOD_START) } else { None },
        schedule_id: None,
        tags: Vec::new(env),
        currency: SorobanString::from_str(env, "XLM"),
    }
}

fn policy(env: &Env, id: u32) -> InsurancePolicy {
    InsurancePolicy {
        id,
        owner: Address::generate(env),
        name: SorobanString::from_str(env, "Policy"),
        coverage_type: CoverageType::Health,
        monthly_premium: 100,
        coverage_amount: 10_000,
        external_ref: None,
        active: true,
        created_at: PERIOD_START,
        last_payment_at: PERIOD_START,
        next_payment_date: PERIOD_END,
    }
}

fn configure_reporting(
    env: &Env,
    split_mode: u32,
    bill_mode: u32,
    insurance_mode: u32,
) -> (ReportingContractClient<'_>, Address) {
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.init(&admin);

    let split_id = env.register_contract(None, availability_split::AvailabilitySplit);
    let savings_id = env.register_contract(None, availability_savings::AvailabilitySavings);
    let bills_id = env.register_contract(None, availability_bills::AvailabilityBills);
    let insurance_id = env.register_contract(None, availability_insurance::AvailabilityInsurance);
    let family_wallet_id = Address::generate(env);

    availability_split::AvailabilitySplitClient::new(env, &split_id).seed(&split_mode);
    availability_bills::AvailabilityBillsClient::new(env, &bills_id).seed(&bill_mode);
    availability_insurance::AvailabilityInsuranceClient::new(env, &insurance_id)
        .seed(&insurance_mode);

    client.configure_addresses(
        &admin,
        &split_id,
        &savings_id,
        &bills_id,
        &insurance_id,
        &family_wallet_id,
    );

    (client, Address::generate(env))
}

mod availability_split {
    use super::*;

    #[contract]
    pub struct AvailabilitySplit;

    #[contractimpl]
    impl AvailabilitySplit {
        pub fn seed(env: Env, mode: u32) {
            env.storage().instance().set(&symbol_short!("MODE"), &mode);
        }
    }

    #[contractimpl]
    impl RemittanceSplitTrait for AvailabilitySplit {
        fn get_split(env: &Env) -> Vec<u32> {
            let mode: u32 = env
                .storage()
                .instance()
                .get(&symbol_short!("MODE"))
                .unwrap_or(MODE_FULL);
            if mode == MODE_FAILING {
                panic!("split unavailable");
            }

            let mut split = Vec::new(env);
            split.push_back(50);
            split.push_back(30);
            split.push_back(15);
            split.push_back(5);
            split
        }

        fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
            let mode: u32 = env
                .storage()
                .instance()
                .get(&symbol_short!("MODE"))
                .unwrap_or(MODE_FULL);
            if mode == MODE_FAILING {
                panic!("split unavailable");
            }

            let mut amounts = Vec::new(&env);
            amounts.push_back(total_amount * 50 / 100);
            amounts.push_back(total_amount * 30 / 100);
            amounts.push_back(total_amount * 15 / 100);
            amounts.push_back(total_amount * 5 / 100);
            amounts
        }
    }
}

mod availability_savings {
    use super::*;

    #[contract]
    pub struct AvailabilitySavings;

    #[contractimpl]
    impl SavingsGoalsTrait for AvailabilitySavings {
        fn get_all_goals(env: Env, _owner: Address) -> Vec<SavingsGoal> {
            Vec::new(&env)
        }

        fn get_goals(env: Env, _owner: Address, _cursor: u32, _limit: u32) -> GoalPage {
            GoalPage {
                items: Vec::new(&env),
                next_cursor: 0,
                count: 0,
            }
        }

        fn is_goal_completed(_env: Env, _goal_id: u32) -> bool {
            false
        }
    }
}

mod availability_bills {
    use super::*;

    #[contract]
    pub struct AvailabilityBills;

    #[contractimpl]
    impl AvailabilityBills {
        pub fn seed(env: Env, mode: u32) {
            env.storage().instance().set(&symbol_short!("MODE"), &mode);
        }
    }

    #[contractimpl]
    impl BillPaymentsTrait for AvailabilityBills {
        fn get_unpaid_bills(env: Env, _owner: Address, _cursor: u32, _limit: u32) -> BillPage {
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
            let mode: u32 = env
                .storage()
                .instance()
                .get(&symbol_short!("MODE"))
                .unwrap_or(MODE_FULL);

            match mode {
                MODE_EMPTY => BillPage {
                    items: Vec::new(&env),
                    next_cursor: 0,
                    count: 0,
                },
                MODE_OVER_LIMIT => {
                    let mut items = Vec::new(&env);
                    items.push_back(bill(&env, owner, cursor + 1, true));
                    BillPage {
                        count: items.len(),
                        items,
                        next_cursor: cursor + 1,
                    }
                }
                _ => {
                    let mut items = Vec::new(&env);
                    items.push_back(bill(&env, owner, 1, true));
                    BillPage {
                        count: items.len(),
                        items,
                        next_cursor: 0,
                    }
                }
            }
        }
    }
}

mod availability_insurance {
    use super::*;

    #[contract]
    pub struct AvailabilityInsurance;

    #[contractimpl]
    impl AvailabilityInsurance {
        pub fn seed(env: Env, mode: u32) {
            env.storage().instance().set(&symbol_short!("MODE"), &mode);
        }
    }

    #[contractimpl]
    impl InsuranceTrait for AvailabilityInsurance {
        fn get_active_policies(env: Env, _owner: Address, cursor: u32, _limit: u32) -> PolicyPage {
            let mode: u32 = env
                .storage()
                .instance()
                .get(&symbol_short!("MODE"))
                .unwrap_or(MODE_FULL);

            match mode {
                MODE_EMPTY => PolicyPage {
                    items: Vec::new(&env),
                    next_cursor: 0,
                    count: 0,
                },
                MODE_OVER_LIMIT => {
                    let mut items = Vec::new(&env);
                    items.push_back(cursor + 1);
                    PolicyPage {
                        count: items.len(),
                        items,
                        next_cursor: cursor + 1,
                    }
                }
                _ => {
                    let mut items = Vec::new(&env);
                    items.push_back(1);
                    PolicyPage {
                        count: items.len(),
                        items,
                        next_cursor: 0,
                    }
                }
            }
        }

        fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy> {
            Some(policy(&env, policy_id))
        }

        fn get_total_monthly_premium(env: Env, _owner: Address) -> i128 {
            let mode: u32 = env
                .storage()
                .instance()
                .get(&symbol_short!("MODE"))
                .unwrap_or(MODE_FULL);
            if mode == MODE_EMPTY {
                0
            } else {
                100
            }
        }
    }
}

#[test]
fn data_availability_reports_complete_with_full_dependency_pages() {
    let env = create_env();
    let (client, user) = configure_reporting(&env, MODE_FULL, MODE_FULL, MODE_FULL);

    let remittance = client.get_remittance_summary(&user, &10_000, &PERIOD_START, &PERIOD_END);
    let bills = client.get_bill_compliance_report(&user, &user, &PERIOD_START, &PERIOD_END);
    let insurance = client.get_insurance_report(&user, &user, &PERIOD_START, &PERIOD_END);
    let health =
        client.get_financial_health_report(&user, &user, &10_000, &PERIOD_START, &PERIOD_END);

    assert_eq!(remittance.data_availability, DataAvailability::Complete);
    assert_eq!(remittance.category_breakdown.len(), 4);
    assert_eq!(
        remittance.category_breakdown.get(0).unwrap().category,
        Category::Spending
    );
    assert_eq!(bills.data_availability, DataAvailability::Complete);
    assert_eq!(bills.total_bills, 1);
    assert_eq!(insurance.data_availability, DataAvailability::Complete);
    assert_eq!(insurance.active_policies, 1);
    assert_eq!(health.data_availability, DataAvailability::Complete);
    assert!(health.health_score.score <= 100);
}

#[test]
fn data_availability_reports_missing_with_zero_dependency_pages() {
    let env = create_env();
    let (client, user) = configure_reporting(&env, MODE_FULL, MODE_EMPTY, MODE_EMPTY);

    let remittance = client.get_remittance_summary(&user, &10_000, &PERIOD_START, &PERIOD_END);
    let bills = client.get_bill_compliance_report(&user, &user, &PERIOD_START, &PERIOD_END);
    let insurance = client.get_insurance_report(&user, &user, &PERIOD_START, &PERIOD_END);
    let health =
        client.get_financial_health_report(&user, &user, &10_000, &PERIOD_START, &PERIOD_END);
    let score = client.calculate_health_score(&user, &10_000);

    assert_eq!(remittance.data_availability, DataAvailability::Complete);
    assert_eq!(bills.data_availability, DataAvailability::Missing);
    assert_eq!(bills.total_bills, 0);
    assert_eq!(insurance.data_availability, DataAvailability::Missing);
    assert_eq!(insurance.active_policies, 0);
    assert_eq!(health.data_availability, DataAvailability::Missing);
    assert_eq!(
        health.bill_compliance.data_availability,
        DataAvailability::Missing
    );
    assert_eq!(
        health.insurance_report.data_availability,
        DataAvailability::Missing
    );
    assert!(score.score <= 100);
    assert!(score.bills_score <= 40);
    assert!(score.insurance_score <= 20);
}

#[test]
fn remittance_summary_missing_without_configured_dependencies() {
    let env = create_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let remittance = client.get_remittance_summary(&user, &10_000, &PERIOD_START, &PERIOD_END);

    assert_eq!(remittance.data_availability, DataAvailability::Missing);
    assert_eq!(remittance.category_breakdown.len(), 0);
}

#[test]
fn remittance_summary_partial_when_split_dependency_fails() {
    let env = create_env();
    let (client, user) = configure_reporting(&env, MODE_FAILING, MODE_FULL, MODE_FULL);

    let remittance = client.get_remittance_summary(&user, &10_000, &PERIOD_START, &PERIOD_END);
    let health =
        client.get_financial_health_report(&user, &user, &10_000, &PERIOD_START, &PERIOD_END);

    assert_eq!(remittance.data_availability, DataAvailability::Partial);
    assert_eq!(
        health.remittance_summary.data_availability,
        DataAvailability::Partial
    );
    assert_eq!(health.data_availability, DataAvailability::Partial);
}

#[test]
fn data_availability_reports_partial_with_over_page_limit_dependencies() {
    let env = create_env();
    let (client, user) = configure_reporting(&env, MODE_FULL, MODE_OVER_LIMIT, MODE_OVER_LIMIT);

    let bills = client.get_bill_compliance_report(&user, &user, &PERIOD_START, &PERIOD_END);
    let insurance = client.get_insurance_report(&user, &user, &PERIOD_START, &PERIOD_END);
    let health =
        client.get_financial_health_report(&user, &user, &10_000, &PERIOD_START, &PERIOD_END);

    assert_eq!(bills.data_availability, DataAvailability::Partial);
    assert_eq!(bills.total_bills, MAX_DEP_PAGES);
    assert_eq!(insurance.data_availability, DataAvailability::Partial);
    assert_eq!(insurance.active_policies, MAX_DEP_PAGES);
    assert_eq!(
        health.bill_compliance.data_availability,
        DataAvailability::Partial
    );
    assert_eq!(
        health.insurance_report.data_availability,
        DataAvailability::Partial
    );
    assert_eq!(health.data_availability, DataAvailability::Partial);
}

#[test]
fn financial_health_report_rolls_up_worst_component_availability() {
    let env = create_env();
    let (client, user) = configure_reporting(&env, MODE_FULL, MODE_OVER_LIMIT, MODE_EMPTY);

    let health =
        client.get_financial_health_report(&user, &user, &10_000, &PERIOD_START, &PERIOD_END);

    assert_eq!(
        health.remittance_summary.data_availability,
        DataAvailability::Complete
    );
    assert_eq!(
        health.bill_compliance.data_availability,
        DataAvailability::Partial
    );
    assert_eq!(
        health.insurance_report.data_availability,
        DataAvailability::Missing
    );
    assert_eq!(
        health.data_availability,
        DataAvailability::Missing,
        "Missing must dominate Partial when the financial health report rolls up component availability"
    );
    assert!(health.health_score.score <= 100);
}
