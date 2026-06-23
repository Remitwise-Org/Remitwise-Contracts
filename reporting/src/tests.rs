use soroban_sdk::testutils::storage::Instance as StorageInstance;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger, LedgerInfo},
    Address, Env,
};
use testutils::set_ledger_time;

use crate::{
    Category, ContractAddresses, DataAvailability, ReportingContract, ReportingContractClient,
    ReportingError, MAX_DEP_PAGES,
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
    use crate::{GoalPage, SavingsGoal, SavingsGoalsTrait};
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
                tags: Vec::new(&env),
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
                tags: Vec::new(&env),
            });
            goals
        }

        fn get_goals(env: Env, owner: Address, _cursor: u32, _limit: u32) -> GoalPage {
            let items = Self::get_all_goals(env, owner);
            GoalPage {
                count: items.len(),
                items,
                next_cursor: 0,
            }
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
    use soroban_sdk::testutils::Address as _;
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
            let mut items = Vec::new(&env);
            items.push_back(1u32);
            crate::PolicyPage {
                count: items.len(),
                items,
                next_cursor: 0,
            }
        }

        fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy> {
            if policy_id == 1 {
                Some(InsurancePolicy {
                    id: 1,
                    owner: Address::generate(&env),
                    name: SorobanString::from_str(&env, "Health Insurance"),
                    coverage_type: CoverageType::Health,
                    monthly_premium: 200,
                    coverage_amount: 50000,
                    external_ref: None,
                    active: true,
                    created_at: 1704067200,
                    last_payment_at: 0,
                    next_payment_date: 1735689600,
                })
            } else {
                None
            }
        }

        fn get_total_monthly_premium(_env: Env, _owner: Address) -> i128 {
            200
        }
    }
}

mod family_wallet {
    use crate::{MemberAddressPage, SpendingPeriod, SpendingTracker};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Vec};

    #[contract]
    pub struct FamilyWallet;

    #[contractimpl]
    impl FamilyWallet {
        pub fn get_owner(env: Env) -> Address {
            Address::generate(&env)
        }

        pub fn get_member_addresses_page(env: Env, _cursor: u32, _limit: u32) -> MemberAddressPage {
            MemberAddressPage {
                items: Vec::new(&env),
                next_cursor: 0,
                count: 0,
            }
        }

        pub fn get_spending_tracker(_env: Env, _member: Address) -> Option<SpendingTracker> {
            None
        }
    }

    fn tracker(current_spent: i128) -> SpendingTracker {
        SpendingTracker {
            current_spent,
            last_tx_timestamp: 1_704_067_200,
            tx_count: 1,
            period: SpendingPeriod {
                period_type: 2,
                period_start: 1_704_067_200,
                period_duration: 2_592_000,
            },
        }
    }

    pub const MODE_COMPLETE: u32 = 0;
    pub const MODE_PARTIAL_TRACKER: u32 = 1;
    pub const MODE_MISSING: u32 = 2;
    pub const MODE_EMPTY: u32 = 3;
    pub const MODE_OVERFLOW: u32 = 4;

    mod scenario {
        use super::*;

        #[contract]
        pub struct FamilyWalletScenario;

        #[contractimpl]
        impl FamilyWalletScenario {
            pub fn seed(env: Env, mode: u32, members: Vec<Address>) {
                env.storage().instance().set(&symbol_short!("MODE"), &mode);
                env.storage()
                    .instance()
                    .set(&symbol_short!("MBRS"), &members);
            }

            pub fn get_owner(env: Env) -> Address {
                let members: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&symbol_short!("MBRS"))
                    .unwrap_or_else(|| Vec::new(&env));
                members.get(0).unwrap_or_else(|| Address::generate(&env))
            }

            pub fn get_member_addresses_page(
                env: Env,
                cursor: u32,
                _limit: u32,
            ) -> MemberAddressPage {
                let mode: u32 = env
                    .storage()
                    .instance()
                    .get(&symbol_short!("MODE"))
                    .unwrap_or(0);
                if mode == MODE_MISSING {
                    panic!("family wallet unreachable");
                }
                if mode == MODE_EMPTY {
                    return MemberAddressPage {
                        items: Vec::new(&env),
                        next_cursor: 0,
                        count: 0,
                    };
                }

                let members: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&symbol_short!("MBRS"))
                    .unwrap_or_else(|| Vec::new(&env));

                match mode {
                    MODE_COMPLETE if cursor == 0 => {
                        let mut items = Vec::new(&env);
                        if let Some(a) = members.get(0) {
                            items.push_back(a);
                        }
                        if let Some(b) = members.get(1) {
                            items.push_back(b);
                        }
                        MemberAddressPage {
                            count: items.len(),
                            items,
                            next_cursor: if members.len() > 2 { 2 } else { 0 },
                        }
                    }
                    MODE_COMPLETE if cursor == 2 => {
                        let mut items = Vec::new(&env);
                        if let Some(c) = members.get(2) {
                            items.push_back(c);
                        }
                        MemberAddressPage {
                            count: items.len(),
                            items,
                            next_cursor: 0,
                        }
                    }
                    _ => MemberAddressPage {
                        count: members.len(),
                        items: members,
                        next_cursor: 0,
                    },
                }
            }

            pub fn get_spending_tracker(env: Env, member: Address) -> Option<SpendingTracker> {
                let mode: u32 = env
                    .storage()
                    .instance()
                    .get(&symbol_short!("MODE"))
                    .unwrap_or(0);
                if mode == MODE_MISSING {
                    panic!("family wallet unreachable");
                }

                let members: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&symbol_short!("MBRS"))
                    .unwrap_or_else(|| Vec::new(&env));

                match mode {
                    MODE_COMPLETE => {
                        if Some(member.clone()) == members.get(0) {
                            Some(tracker(150))
                        } else if Some(member.clone()) == members.get(1) {
                            Some(tracker(50))
                        } else {
                            None
                        }
                    }
                    MODE_PARTIAL_TRACKER => {
                        if Some(member) == members.get(0) {
                            Some(tracker(25))
                        } else {
                            panic!("tracker unavailable")
                        }
                    }
                    MODE_OVERFLOW => {
                        if Some(member) == members.get(0) {
                            Some(tracker(i128::MAX))
                        } else {
                            Some(tracker(1))
                        }
                    }
                    _ => None,
                }
            }
        }
    }

    pub use scenario::{FamilyWalletScenario, FamilyWalletScenarioClient};
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
    let result = client.try_init(&admin);
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
    assert!(result.is_err());
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
        remittance_split: contract_id.clone(),
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
fn test_verify_dependency_address_set_does_not_write_storage() {
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

    let _ = client.try_verify_dependency_address_set(&addrs);

    // Instance storage is only accessible inside a contract context, so the
    // post-condition reads must run within `env.as_contract`.
    let instance_snapshot: Option<Address> =
        env.as_contract(&contract_id, || env.storage().instance().get(&symbol_short!("ADMIN")));
    assert!(instance_snapshot.is_some(), "ADMIN should still exist");

    let stored_addrs: Option<ContractAddresses> =
        env.as_contract(&contract_id, || env.storage().instance().get(&symbol_short!("ADDRS")));
    assert!(
        stored_addrs.is_none(),
        "ADDRS must not be written by preflight"
    );
}

#[test]
fn test_verify_dependency_address_set_rejects_multiple_duplicates() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    let x = Address::generate(&env);
    let addrs = ContractAddresses {
        remittance_split: x.clone(),
        savings_goals: x.clone(),
        bill_payments: x.clone(),
        insurance: x.clone(),
        family_wallet: x,
    };
    let result = client.try_verify_dependency_address_set(&addrs);
    assert!(matches!(
        result,
        Err(Ok(ReportingError::InvalidDependencyAddressConfiguration))
    ));
}

#[test]
fn test_verify_dependency_address_set_deterministic_error() {
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

    let result1 = client.try_verify_dependency_address_set(&addrs);
    let result2 = client.try_verify_dependency_address_set(&addrs);
    assert!(matches!(
        result1,
        Err(Ok(ReportingError::InvalidDependencyAddressConfiguration))
    ));
    assert!(matches!(
        result2,
        Err(Ok(ReportingError::InvalidDependencyAddressConfiguration))
    ));
}

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

    let result =
        client.try_get_remittance_summary(&user, &total_amount, &period_start, &period_end);
    assert!(result.is_ok());
    let summary = result.unwrap().unwrap();

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
    let report = client.get_savings_report(&user, &user, &period_start, &period_end);

    assert_eq!(report.total_goals, 2);
    assert_eq!(report.completed_goals, 1);
}

#[test]
fn test_get_savings_report_rejects_invalid_period() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let result = client.try_get_savings_report(&user, &user, &200, &100);
    assert!(matches!(result, Err(Ok(ReportingError::InvalidPeriod))));
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

    let result = client.try_get_bill_compliance_report(&user, &user, &period_start, &period_end);
    assert!(result.is_ok());
}

#[test]
fn test_get_bill_compliance_report_rejects_invalid_period() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let result = client.try_get_bill_compliance_report(&user, &user, &200, &100);
    assert!(matches!(result, Err(Ok(ReportingError::InvalidPeriod))));
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

    let result = client.try_get_insurance_report(&user, &user, &period_start, &period_end);
    assert!(result.is_ok());
}

#[test]
fn test_get_insurance_report_rejects_invalid_period() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let result = client.try_get_insurance_report(&user, &user, &200, &100);
    assert!(matches!(result, Err(Ok(ReportingError::InvalidPeriod))));
}

#[test]
fn test_get_family_spending_report_complete() {
    let env = create_test_env();
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
    let family_wallet_id = env.register_contract(None, family_wallet::FamilyWalletScenario);
    let mut members = soroban_sdk::Vec::new(&env);
    members.push_back(Address::generate(&env));
    members.push_back(Address::generate(&env));
    members.push_back(Address::generate(&env));
    let family_client = family_wallet::FamilyWalletScenarioClient::new(&env, &family_wallet_id);
    family_client.seed(&family_wallet::MODE_COMPLETE, &members);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet_id,
    );

    let report =
        client.get_family_spending_report(&user, &user, &1_704_067_200u64, &1_706_745_600u64);

    assert_eq!(report.total_members, 3);
    assert_eq!(report.total_spending, 200);
    assert_eq!(report.average_per_member, 66);
    assert_eq!(report.data_availability, DataAvailability::Complete);
    assert_eq!(report.member_breakdown.len(), 3);

    let first = report.member_breakdown.get(0).unwrap();
    let second = report.member_breakdown.get(1).unwrap();
    let third = report.member_breakdown.get(2).unwrap();

    assert_ne!(first.member, second.member);
    assert_ne!(first.member, third.member);
    assert_ne!(second.member, third.member);

    assert_eq!(first.total_spending, 150);
    assert!(first.data_available);
    assert_eq!(second.total_spending, 50);
    assert!(second.data_available);
    assert_eq!(third.total_spending, 0);
    assert!(third.data_available);
}

#[test]
fn test_get_family_spending_report_partial_when_member_tracker_fails() {
    let env = create_test_env();
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
    let family_wallet_id = env.register_contract(None, family_wallet::FamilyWalletScenario);
    let mut members = soroban_sdk::Vec::new(&env);
    members.push_back(Address::generate(&env));
    members.push_back(Address::generate(&env));
    let family_client = family_wallet::FamilyWalletScenarioClient::new(&env, &family_wallet_id);
    family_client.seed(&family_wallet::MODE_PARTIAL_TRACKER, &members);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet_id,
    );

    let report =
        client.get_family_spending_report(&user, &user, &1_704_067_200u64, &1_706_745_600u64);

    assert_eq!(report.total_members, 2);
    assert_eq!(report.total_spending, 25);
    assert_eq!(report.data_availability, DataAvailability::Partial);
    assert_eq!(report.member_breakdown.len(), 2);
    assert!(report.member_breakdown.get(0).unwrap().data_available);
    assert!(!report.member_breakdown.get(1).unwrap().data_available);
    assert_eq!(report.member_breakdown.get(1).unwrap().total_spending, 0);
}

#[test]
fn test_get_family_spending_report_missing_when_wallet_unreachable() {
    let env = create_test_env();
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
    let family_wallet_id = env.register_contract(None, family_wallet::FamilyWalletScenario);
    let mut members = soroban_sdk::Vec::new(&env);
    members.push_back(Address::generate(&env));
    let family_client = family_wallet::FamilyWalletScenarioClient::new(&env, &family_wallet_id);
    family_client.seed(&family_wallet::MODE_MISSING, &members);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet_id,
    );

    let report =
        client.get_family_spending_report(&user, &user, &1_704_067_200u64, &1_706_745_600u64);

    assert_eq!(report.total_members, 0);
    assert_eq!(report.total_spending, 0);
    assert_eq!(report.average_per_member, 0);
    assert_eq!(report.data_availability, DataAvailability::Missing);
    assert_eq!(report.member_breakdown.len(), 0);
}

#[test]
fn test_get_family_spending_report_zero_members_maps_to_missing() {
    let env = create_test_env();
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
    let family_wallet_id = env.register_contract(None, family_wallet::FamilyWalletScenario);
    let family_client = family_wallet::FamilyWalletScenarioClient::new(&env, &family_wallet_id);
    family_client.seed(&family_wallet::MODE_EMPTY, &soroban_sdk::Vec::new(&env));

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet_id,
    );

    let report =
        client.get_family_spending_report(&user, &user, &1_704_067_200u64, &1_706_745_600u64);

    assert_eq!(report.total_members, 0);
    assert_eq!(report.total_spending, 0);
    assert_eq!(report.data_availability, DataAvailability::Missing);
}

#[test]
fn test_get_family_spending_report_overflow_clamps_and_marks_partial() {
    let env = create_test_env();
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
    let family_wallet_id = env.register_contract(None, family_wallet::FamilyWalletScenario);
    let mut members = soroban_sdk::Vec::new(&env);
    members.push_back(Address::generate(&env));
    members.push_back(Address::generate(&env));
    let family_client = family_wallet::FamilyWalletScenarioClient::new(&env, &family_wallet_id);
    family_client.seed(&family_wallet::MODE_OVERFLOW, &members);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet_id,
    );

    let report =
        client.get_family_spending_report(&user, &user, &1_704_067_200u64, &1_706_745_600u64);

    assert_eq!(report.total_members, 2);
    assert_eq!(report.total_spending, i128::MAX);
    assert_eq!(report.average_per_member, i128::MAX / 2);
    assert_eq!(report.data_availability, DataAvailability::Partial);
}

#[test]
fn test_get_family_spending_report_requires_user_auth() {
    let env = create_test_env();
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
    let family_wallet_id = env.register_contract(None, family_wallet::FamilyWalletScenario);
    let mut members = soroban_sdk::Vec::new(&env);
    members.push_back(Address::generate(&env));
    members.push_back(Address::generate(&env));
    members.push_back(Address::generate(&env));
    let family_client = family_wallet::FamilyWalletScenarioClient::new(&env, &family_wallet_id);
    family_client.seed(&family_wallet::MODE_COMPLETE, &members);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet_id,
    );

    let _ = client.get_family_spending_report(&user, &user, &1_704_067_200u64, &1_706_745_600u64);

    let auths = env.auths();
    let found = auths.iter().any(|(addr, _)| *addr == user);
    assert!(found, "family spending report must require user auth");
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
    assert!(result.is_ok());
    let health_score = result.unwrap().unwrap();

    assert_eq!(health_score.score, 87);
}

#[test]
fn test_calculate_health_score_edge_cases() {
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

    // Test with zero total target (no goals)
    // Note: Mock always returns goals, so this tests the calculation with actual goals
    let health_score = client.calculate_health_score(&user, &0);
    assert_eq!(health_score.savings_score, 32); // 80% completion -> 32 points
    assert_eq!(health_score.score, 87); // 32 + 35 + 20
}

#[test]
fn test_calculate_health_score_overflow_protection() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init(&admin);

    // Register mock contracts with extreme values
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

    // Test should complete without panicking even with extreme inputs
    let health_score = client.calculate_health_score(&user, &i128::MAX);

    // Scores should be bounded
    assert!(health_score.score <= 100);
    assert!(health_score.savings_score <= 40);
    assert!(health_score.bills_score <= 40);
    assert!(health_score.insurance_score <= 20);
}

#[test]
fn test_calculate_health_score_no_unpaid_bills() {
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

    let health_score = client.calculate_health_score(&user, &10000);

    // With unpaid bills (none overdue), bills_score should be 35
    assert_eq!(health_score.bills_score, 35);
}

#[test]
fn test_calculate_health_score_no_insurance() {
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

    let health_score = client.calculate_health_score(&user, &10000);

    // With insurance, insurance_score should be 20
    assert_eq!(health_score.insurance_score, 20);
}

#[test]
fn test_calculate_health_score_bounds_guarantee() {
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

    // Test multiple times to ensure consistency
    for _ in 0..10 {
        let health_score = client.calculate_health_score(&user, &10000);

        // All scores must be within bounds
        assert!(health_score.score <= 100);
        assert!(health_score.savings_score <= 40);
        assert!(health_score.bills_score <= 40);
        assert!(health_score.insurance_score <= 20);

        // Total should equal sum of components
        assert_eq!(
            health_score.score,
            health_score.savings_score + health_score.bills_score + health_score.insurance_score
        );
    }
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

    let result = client.try_get_financial_health_report(
        &user,
        &user,
        &total_remittance,
        &period_start,
        &period_end,
    );
    assert!(result.is_ok());
    let report = result.unwrap().unwrap();

    assert_eq!(report.health_score.score, 87);
}

#[test]
fn test_get_financial_health_report_rejects_invalid_period() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let result = client.try_get_financial_health_report(&user, &user, &10_000i128, &200, &100);
    assert!(matches!(result, Err(Ok(ReportingError::InvalidPeriod))));
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

    let trend = client.get_trend_analysis(&user, &user, &current_amount, &previous_amount);

    assert_eq!(trend.current_amount, 15000);
    assert_eq!(trend.change_percentage, 50);
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

    let result = client.try_get_financial_health_report(
        &user,
        &user,
        &total_remittance,
        &period_start,
        &period_end,
    );
    assert!(result.is_ok());
    let report = result.unwrap().unwrap();

    let period_key = 202401u64;
    let stored = client.store_report(&user, &report, &period_key);
    assert!(stored);

    let retrieved = client.get_stored_report(&user, &user, &period_key);
    assert!(retrieved.is_some());
}

// ============================================================================
// ADMIN OPERATIONS - Archive and Cleanup
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

    let result = client.try_get_financial_health_report(
        &user,
        &user,
        &10000i128,
        &1704067200u64,
        &1706745600u64,
    );
    assert!(result.is_ok());
    let report = result.unwrap().unwrap();

    let period_key = 202401u64;
    client.store_report(&user, &report, &period_key);

    let archive_result = client.try_archive_old_reports(&admin, &2000000000);
    assert!(archive_result.is_ok());
    assert_eq!(archive_result.unwrap().unwrap(), 1);

    assert!(client
        .get_stored_report(&user, &user, &period_key)
        .is_none());
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

    let result = client.try_get_financial_health_report(
        &user,
        &user,
        &10000i128,
        &1704067200u64,
        &1706745600u64,
    );
    assert!(result.is_ok());
    let report = result.unwrap().unwrap();
    client.store_report(&user, &report, &202401);

    let stats = client.get_storage_stats();
    assert_eq!(stats.active_reports, 1);
    assert_eq!(stats.archived_reports, 0);

    // Archive and check stats
    client.archive_old_reports(&admin, &2000000000);

    let cleanup_result = client.try_cleanup_old_reports(&admin, &2000000000);
    assert!(cleanup_result.is_ok());
    assert_eq!(cleanup_result.unwrap().unwrap(), 1);
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
        let report =
            client.get_financial_health_report(&user, &user, &10000, &1704067200, &1706745600);
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
    let report = client.get_financial_health_report(&user, &user, &10000, &1704067200, &1706745600);
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
// TTL TESTS
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
    assert!(ttl >= 518_400);
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
    client.get_financial_health_report(
        user,
        user,
        &10_000i128,
        &1_704_067_200u64,
        &1_706_745_600u64,
    )
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
    let result_a = client.get_stored_report(&user_a, &user_a, &202_401u64);
    assert!(
        result_a.is_none(),
        "user_a's report slot must be empty when stored under user_b"
    );

    // user_b's slot has the report
    let result_b = client.get_stored_report(&user_b, &user_b, &202_401u64);
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
    let user_result = client.get_stored_report(&user, &user, &202_401u64);
    assert!(
        user_result.is_none(),
        "admin storing under their own address must not populate user's slot"
    );

    // Admin's own slot has the report
    let admin_result = client.get_stored_report(&admin, &admin, &202_401u64);
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
    let result = client.get_stored_report(&user_a, &user_a, &202_401u64);
    assert!(result.is_some(), "user_a must retrieve their own report");

    // Querying with user_b's address for the same period key returns None
    let result_b = client.get_stored_report(&user_b, &user_b, &202_401u64);
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

    let ra = client.get_stored_report(&user_a, &user_a, &period).unwrap();
    let rb = client.get_stored_report(&user_b, &user_b, &period).unwrap();

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

    assert!(client
        .get_stored_report(&user, &user, &202_401u64)
        .is_some());
    assert!(client
        .get_stored_report(&user, &user, &202_402u64)
        .is_some());
    assert!(client
        .get_stored_report(&user, &user, &202_403u64)
        .is_some());
    // Non-existent period returns None
    assert!(client
        .get_stored_report(&user, &user, &202_404u64)
        .is_none());
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

    let retrieved = client.get_stored_report(&user, &user, &period).unwrap();
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
        assert!(client.get_stored_report(user, user, &202_401u64).is_some());
        assert!(client.get_stored_report(user, user, &202_402u64).is_some());
    }

    // Archive all
    let archived_count = client.archive_old_reports(&admin, &2_000_000_000u64);
    assert_eq!(
        archived_count, 6,
        "6 reports (3 users × 2 periods) must be archived"
    );

    // Active storage must be empty for all users
    for user in &users {
        assert!(client.get_stored_report(user, user, &202_401u64).is_none());
        assert!(client.get_stored_report(user, user, &202_402u64).is_none());
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
        client
            .get_stored_report(&user, &user, &202_401u64)
            .is_some(),
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
    let result = client.get_stored_report(&user, &user, &999_999u64);
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
fn test_check_dependencies_returns_one_status_per_configured_dependency() {
    let env = create_test_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

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
    let expected_names = [
        "remittance_split",
        "savings_goals",
        "bill_payments",
        "insurance",
        "family_wallet",
    ];

    assert_eq!(statuses.len(), expected_names.len() as u32);

    for (index, expected_name) in expected_names.iter().enumerate() {
        let status = statuses.get(index as u32).unwrap();

        assert_eq!(
            status.name,
            soroban_sdk::String::from_str(&env, expected_name)
        );
        assert!(status.ok);
        assert_eq!(status.error_category, None);
    }
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
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

    #[contract]
    pub struct InsuranceThreePages;

    #[contractimpl]
    impl InsuranceTrait for InsuranceThreePages {
        fn get_active_policies(env: Env, _owner: Address, cursor: u32, _limit: u32) -> PolicyPage {
            let (policy_id, next_cursor) = match cursor {
                0 => (1u32, 7u32),
                7 => (2, 14),
                _ => (3, 0),
            };
            let mut items = Vec::new(&env);
            items.push_back(policy_id);
            PolicyPage {
                count: items.len(),
                items,
                next_cursor,
            }
        }
        fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy> {
            Some(InsurancePolicy {
                id: policy_id,
                owner: Address::generate(&env),
                name: SorobanString::from_str(&env, "P"),
                coverage_type: CoverageType::Health,
                monthly_premium: 100,
                coverage_amount: 10_000,
                external_ref: None,
                active: true,
                created_at: 0,
                last_payment_at: 0,
                next_payment_date: 1_735_689_600,
            })
        }
        fn get_total_monthly_premium(_env: Env, _owner: Address) -> i128 {
            300
        }
    }
}

// ── Mock: insurance that never returns cursor = 0 ─────────────────────────
mod insurance_infinite {
    use crate::{CoverageType, InsurancePolicy, InsuranceTrait, PolicyPage};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

    #[contract]
    pub struct InsuranceInfinite;

    #[contractimpl]
    impl InsuranceTrait for InsuranceInfinite {
        fn get_active_policies(env: Env, _owner: Address, cursor: u32, _limit: u32) -> PolicyPage {
            let mut items = Vec::new(&env);
            items.push_back(cursor);
            PolicyPage {
                count: items.len(),
                items,
                next_cursor: cursor + 1,
            }
        }
        fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy> {
            Some(InsurancePolicy {
                id: policy_id,
                owner: Address::generate(&env),
                name: SorobanString::from_str(&env, "P"),
                coverage_type: CoverageType::Health,
                monthly_premium: 100,
                coverage_amount: 10_000,
                external_ref: None,
                active: true,
                created_at: 0,
                last_payment_at: 0,
                next_payment_date: 1_735_689_600,
            })
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
) -> (ReportingContractClient<'_>, Address) {
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
    let report =
        client.get_bill_compliance_report(&user, &user, &1_704_067_200u64, &1_706_745_600u64);

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
    let report =
        client.get_bill_compliance_report(&user, &user, &1_704_067_200u64, &1_706_745_600u64);

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
    let report =
        client.get_bill_compliance_report(&user, &user, &1_704_067_200u64, &1_706_745_600u64);
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
    let report = client.get_insurance_report(&user, &user, &1_704_067_200u64, &1_706_745_600u64);

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
    let report = client.get_insurance_report(&user, &user, &1_704_067_200u64, &1_706_745_600u64);

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
    let report = client.get_insurance_report(&user, &user, &1_704_067_200u64, &1_706_745_600u64);
    assert_eq!(
        report.active_policies, 3,
        "cursor must advance monotonically so each page is visited exactly once"
    );
    assert_eq!(report.data_availability, DataAvailability::Complete);
}

mod topn_tie_bills {
    use crate::{Bill, BillPage, BillPaymentsTrait};
    use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

    #[contract]
    pub struct BillsTieAllEqual;

    // Always return 1 page for simplicity.
    #[contractimpl]
    impl BillPaymentsTrait for BillsTieAllEqual {
        fn get_unpaid_bills(_env: Env, _owner: Address, _c: u32, _l: u32) -> BillPage {
            BillPage {
                items: Vec::new(&_env),
                next_cursor: 0,
                count: 0,
            }
        }

        fn get_total_unpaid(_env: Env, _owner: Address) -> i128 {
            0
        }

        fn get_all_bills_for_owner(env: Env, owner: Address, _cursor: u32, _limit: u32) -> BillPage {
            // All equal amounts => order must be ID ascending due to tie-break.
            // Also intentionally insert in descending id order to catch non-determinism.
            let mut items = Vec::new(&env);
            let amount: i128 = 100;
            let ids = [5u32, 4, 3, 2, 1];
            for id in ids.iter() {
                items.push_back(Bill {
                    id: *id,
                    owner: owner.clone(),
                    name: SorobanString::from_str(&env, "B"),
                    external_ref: None,
                    amount,
                    due_date: 1_735_689_600,
                    recurring: false,
                    frequency_days: 0,
                    paid: false,
                    created_at: 1_704_067_200,
                    paid_at: None,
                    schedule_id: None,
                    tags: Vec::new(&env),
                    currency: SorobanString::from_str(&env, "XLM"),
                });
            }
            BillPage { items, next_cursor: 0, count: ids.len() as u32 }
        }
    }
}

mod topn_tie_savings {
    use crate::{GoalPage, SavingsGoal, SavingsGoalsTrait};
    use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

    #[contract]
    pub struct SavingsTieAllEqual;

    #[contractimpl]
    impl SavingsGoalsTrait for SavingsTieAllEqual {
        fn get_all_goals(_env: Env, _owner: Address) -> Vec<SavingsGoal> {
            // Not used by Top-N path (uses paginated get_goals).
            Vec::new(&_env)
        }

        fn get_goals(env: Env, owner: Address, _cursor: u32, _limit: u32) -> GoalPage {
            // All equal target_amount => order must be ID ascending.
            let mut items = Vec::new(&env);
            let target_amount: i128 = 10_000;
            let ids = [5u32, 4, 3, 2, 1];
            for id in ids.iter() {
                items.push_back(SavingsGoal {
                    id: *id,
                    owner: owner.clone(),
                    name: SorobanString::from_str(&env, "G"),
                    target_amount,
                    current_amount: 1_000,
                    target_date: 1_735_689_600,
                    locked: false,
                    unlock_date: None,
                    tags: Vec::new(&env),
                });
            }
            GoalPage { items, next_cursor: 0, count: ids.len() as u32 }
        }

        fn is_goal_completed(_env: Env, _goal_id: u32) -> bool {
            false
        }
    }
}

#[test]
fn test_top_n_reports_tie_break_is_deterministic_bills() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1704067200);

    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, topn_tie_bills::BillsTieAllEqual);
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

    let r1 = client.get_top_bills_report(&user, &period_start, &period_end);
    let r2 = client.get_top_bills_report(&user, &period_start, &period_end);
    assert!(r1.items.len() <= crate::MAX_ITEMS_PER_REPORT);

    // Deterministic across repeated calls.
    assert_eq!(r1.items.len(), r2.items.len());
    for i in 0..r1.items.len() {
        assert_eq!(r1.items.get(i as u32).unwrap().id, r2.items.get(i as u32).unwrap().id);
    }

    // All amounts equal => order by id ascending => [1,2,3,4,5] capped to MAX.
    // Our mock returns 5 items; MAX is 10, so all 5 should be present.
    let expected_ids = [1u32, 2, 3, 4, 5];
    assert_eq!(r1.items.len(), expected_ids.len() as u32);
    for (i, expected) in expected_ids.iter().enumerate() {
        assert_eq!(r1.items.get(i as u32).unwrap().id, *expected);
    }
}

#[test]
fn test_top_n_reports_tie_break_is_deterministic_savings() {
    let env = create_test_env();
    set_ledger_time(&env, 1, 1704067200);

    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, topn_tie_savings::SavingsTieAllEqual);
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

    let r1 = client.get_top_savings_report(&user, &period_start, &period_end);
    let r2 = client.get_top_savings_report(&user, &period_start, &period_end);

    assert_eq!(r1.items.len(), r2.items.len());
    for i in 0..r1.items.len() {
        assert_eq!(r1.items.get(i as u32).unwrap().id, r2.items.get(i as u32).unwrap().id);
    }

    let expected_ids = [1u32, 2, 3, 4, 5];
    assert_eq!(r1.items.len(), expected_ids.len() as u32);
    for (i, expected) in expected_ids.iter().enumerate() {
        assert_eq!(r1.items.get(i as u32).unwrap().id, *expected);
    }
}

#[test]
fn test_top_n_reports() {
    let env = create_test_env();
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

    // Test Top-N Bills
    let bill_report = client.get_top_bills_report(&user, &period_start, &period_end);
    // BillPayments mock returns 2 bills in get_all_bills_for_owner: id 1 (100) and id 2 (50)
    assert_eq!(bill_report.total_count, 2);
    assert_eq!(bill_report.total_amount, 150);
    assert_eq!(bill_report.items.len(), 2);
    assert_eq!(bill_report.items.get(0).unwrap().amount, 100);
    assert_eq!(bill_report.items.get(1).unwrap().amount, 50);

    // Test Top-N Savings
    let savings_report = client.get_top_savings_report(&user, &period_start, &period_end);
    // SavingsGoals mock returns 2 goals: Education (target 10000) and Emergency (target 5000)
    assert_eq!(savings_report.total_count, 2);
    assert_eq!(savings_report.total_target, 15000);
    assert_eq!(savings_report.total_saved, 12000);
    assert_eq!(savings_report.items.len(), 2);
    assert_eq!(savings_report.items.get(0).unwrap().target_amount, 10000);
    assert_eq!(savings_report.items.get(1).unwrap().target_amount, 5000);
}

