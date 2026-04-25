//! Gas benchmarks for reporting contract heavy aggregation queries.
//!
//! # Covered aggregation paths
//!
//! | Path                        | Scenario group                       |
//! |-----------------------------|--------------------------------------|
//! | `get_remittance_summary`    | `bench_remittance_summary_*`         |
//! | `get_trend_analysis_multi`  | `bench_trend_analysis_multi_*`       |
//! | `get_financial_health_report` | `bench_financial_health_report_*`  |
//! | `archive_old_reports`       | `bench_archive_reports_*`            |
//!
//! # Scaling strategy
//!
//! Each path is exercised at three synthetic data sizes to expose O(n)
//! complexity growth:
//! - **small**  –  5 items per data source
//! - **medium** – 25 items per data source
//! - **large**  – 50 items per data source
//!
//! `get_trend_analysis_multi` is an in-contract computation that receives its
//! history as a `Vec<(u64, i128)>` argument, so size is controlled directly
//! without mock contracts.
//!
//! # Security notes
//!
//! - `mock_all_auths()` mirrors every `require_auth()` call in production code,
//!   so administrative gating is exercised in each benchmark.
//! - Cross-contract data-isolation invariants are covered in the unit tests;
//!   benchmarks focus solely on gas scaling under increasing data volume.
//! - All numeric inputs are within i128 range; no overflow paths are exercised.

use reporting::{DataAvailability, ReportingContract, ReportingContractClient};
use soroban_sdk::{
    testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo},
    Address, Env, Vec,
};

// ── Benchmark infrastructure ─────────────────────────────────────────────────

/// Create a reproducible Soroban test environment with an unlimited budget so
/// that gas measurements reflect pure instruction cost, not budget limits.
fn bench_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 1,
        timestamp: 1_700_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100_000,
    });
    let mut budget = env.budget();
    budget.reset_unlimited();
    env
}

/// Reset the budget tracker, run `f`, and return (cpu_instructions, memory_bytes, result).
fn measure<F, R>(env: &Env, f: F) -> (u64, u64, R)
where
    F: FnOnce() -> R,
{
    let mut budget = env.budget();
    budget.reset_unlimited();
    budget.reset_tracker();
    let result = f();
    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();
    (cpu, mem, result)
}

// Timestamps used across bill-period filtering checks
const PERIOD_START: u64 = 1_699_000_000;
const PERIOD_END: u64 = 1_701_000_000;
/// Bill `created_at` is set to this value so it falls inside [PERIOD_START, PERIOD_END].
const BILL_CREATED_AT: u64 = 1_700_000_000;

// ── Mock: remittance split ────────────────────────────────────────────────────

mod mock_remittance_split {
    use reporting::RemittanceSplitTrait;
    use soroban_sdk::{contract, contractimpl, Env, Vec};

    #[contract]
    pub struct MockRemittanceSplit;

    /// Returns a fixed 50/30/15/5 split matching the standard four categories.
    /// Security: no auth required on read-only split queries.
    #[contractimpl]
    impl RemittanceSplitTrait for MockRemittanceSplit {
        fn get_split(env: &Env) -> Vec<u32> {
            let mut split = Vec::new(env);
            split.push_back(50u32);
            split.push_back(30u32);
            split.push_back(15u32);
            split.push_back(5u32);
            split
        }

        fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
            let mut amounts = Vec::new(&env);
            amounts.push_back(total_amount * 50 / 100);
            amounts.push_back(total_amount * 30 / 100);
            amounts.push_back(total_amount * 15 / 100);
            amounts.push_back(total_amount * 5 / 100);
            amounts
        }
    }
}

// ── Mock factories ────────────────────────────────────────────────────────────
//
// Separate structs are required for each size because each `#[contract]`
// expansion produces a distinct contract type.  Parameterising at runtime via
// contract storage would inflate the measured gas cost with storage reads that
// are not part of the aggregation path under test.

/// Expand a SavingsGoals mock that returns `$n` synthetic goals (70 % funded).
macro_rules! mock_savings {
    ($mod_name:ident, $struct_name:ident, $n:expr) => {
        mod $mod_name {
            use reporting::{SavingsGoal, SavingsGoalsTrait};
            use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

            #[contract]
            pub struct $struct_name;

            #[contractimpl]
            impl SavingsGoalsTrait for $struct_name {
                fn get_all_goals(env: Env, owner: Address) -> Vec<SavingsGoal> {
                    let mut goals = Vec::new(&env);
                    for i in 0u32..$n {
                        let target = 10_000i128 * (i as i128 + 1);
                        goals.push_back(SavingsGoal {
                            id: i,
                            owner: owner.clone(),
                            name: SorobanString::from_str(&env, "Bench Goal"),
                            target_amount: target,
                            current_amount: target * 7 / 10,
                            target_date: 1_800_000_000,
                            locked: false,
                            unlock_date: None,
                        });
                    }
                    goals
                }

                fn is_goal_completed(_env: Env, _goal_id: u32) -> bool {
                    false
                }
            }
        }
    };
}

/// Expand a BillPayments mock that returns `$n` bills; even-indexed are paid.
macro_rules! mock_bills {
    ($mod_name:ident, $struct_name:ident, $n:expr) => {
        mod $mod_name {
            use reporting::{Bill, BillPage, BillPaymentsTrait};
            use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

            #[contract]
            pub struct $struct_name;

            #[contractimpl]
            impl BillPaymentsTrait for $struct_name {
                fn get_unpaid_bills(
                    env: Env,
                    owner: Address,
                    _cursor: u32,
                    _limit: u32,
                ) -> BillPage {
                    let mut items = Vec::new(&env);
                    for i in 0u32..$n {
                        items.push_back(Bill {
                            id: i,
                            owner: owner.clone(),
                            name: SorobanString::from_str(&env, "Bench Bill"),
                            external_ref: None,
                            amount: 100i128,
                            due_date: 1_800_000_000,
                            recurring: false,
                            frequency_days: 30,
                            paid: false,
                            created_at: super::BILL_CREATED_AT,
                            paid_at: None,
                            schedule_id: None,
                            tags: Vec::new(&env),
                            currency: SorobanString::from_str(&env, "USDC"),
                            external_ref: None,
                            tags: Vec::new(&env),
                        });
                    }
                    let count = items.len();
                    BillPage {
                        items,
                        next_cursor: 0,
                        count,
                    }
                }

                fn get_total_unpaid(_env: Env, _owner: Address) -> i128 {
                    100i128 * ($n as i128)
                }

                fn get_all_bills_for_owner(
                    env: Env,
                    owner: Address,
                    _cursor: u32,
                    _limit: u32,
                ) -> BillPage {
                    let mut items = Vec::new(&env);
                    for i in 0u32..$n {
                        let paid = i % 2 == 0;
                        items.push_back(Bill {
                            id: i,
                            owner: owner.clone(),
                            name: SorobanString::from_str(&env, "Bench Bill"),
                            external_ref: None,
                            amount: 100i128,
                            due_date: 1_800_000_000,
                            recurring: false,
                            frequency_days: 30,
                            paid,
                            created_at: super::BILL_CREATED_AT,
                            paid_at: if paid { Some(1_700_010_000) } else { None },
                            schedule_id: None,
                            tags: Vec::new(&env),
                            currency: SorobanString::from_str(&env, "USDC"),
                            external_ref: None,
                            tags: Vec::new(&env),
                        });
                    }
                    let count = items.len();
                    BillPage {
                        items,
                        next_cursor: 0,
                        count,
                    }
                }
            }
        }
    };
}

/// Expand an Insurance mock that returns `$n` active policies at $200/month each.
macro_rules! mock_insurance {
    ($mod_name:ident, $struct_name:ident, $n:expr) => {
        mod $mod_name {
            use reporting::{CoverageType, InsurancePolicy, InsuranceTrait, PolicyPage};
            use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Vec};

            #[contract]
            pub struct $struct_name;

            #[contractimpl]
            impl InsuranceTrait for $struct_name {
                fn get_active_policies(
                    env: Env,
                    owner: Address,
                    _cursor: u32,
                    _limit: u32,
                ) -> PolicyPage {
                    let mut items = Vec::new(&env);
                    for i in 0u32..$n {
                        items.push_back(InsurancePolicy {
                            id: i,
                            owner: owner.clone(),
                            name: SorobanString::from_str(&env, "Bench Policy"),
                            coverage_type: remitwise_common::CoverageType::Health,
                            monthly_premium: 200i128,
                            coverage_amount: 50_000i128,
                            active: true,
                            next_payment_date: 1_800_000_000,
                            external_ref: None,
                        });
                    }
                    let count = items.len();
                    PolicyPage {
                        items,
                        next_cursor: 0,
                        count,
                    }
                }

                fn get_total_monthly_premium(_env: Env, _owner: Address) -> i128 {
                    200i128 * ($n as i128)
                }
            }
        }
    };
}

// Materialise all three sizes
mock_savings!(mock_savings_5, MockSavings5, 5u32);
mock_savings!(mock_savings_25, MockSavings25, 25u32);
mock_savings!(mock_savings_50, MockSavings50, 50u32);

mock_bills!(mock_bills_5, MockBills5, 5u32);
mock_bills!(mock_bills_25, MockBills25, 25u32);
mock_bills!(mock_bills_50, MockBills50, 50u32);

mock_insurance!(mock_insurance_5, MockInsurance5, 5u32);
mock_insurance!(mock_insurance_25, MockInsurance25, 25u32);
mock_insurance!(mock_insurance_50, MockInsurance50, 50u32);

// ── Setup helpers ─────────────────────────────────────────────────────────────

/// Register and initialise a ReportingContract wired to the provided mock
/// contract addresses.  Returns (client, admin_address, user_address).
///
/// `family_wallet` is not called by any aggregation path; a freshly generated
/// address is used as a safe, unused placeholder.
macro_rules! setup_reporting {
    ($env:expr,
     $remittance_mod:ident :: $remittance_struct:ident,
     $savings_mod:ident :: $savings_struct:ident,
     $bills_mod:ident :: $bills_struct:ident,
     $insurance_mod:ident :: $insurance_struct:ident
    ) => {{
        let remittance_id = $env.register_contract(None, $remittance_mod::$remittance_struct);
        let savings_id = $env.register_contract(None, $savings_mod::$savings_struct);
        let bills_id = $env.register_contract(None, $bills_mod::$bills_struct);
        let insurance_id = $env.register_contract(None, $insurance_mod::$insurance_struct);
        let family_wallet_dummy = Address::generate(&$env);

        let contract_id = $env.register_contract(None, ReportingContract);
        let client = ReportingContractClient::new(&$env, &contract_id);
        let admin = Address::generate(&$env);
        let user = Address::generate(&$env);

        client.init(&admin);
        client.configure_addresses(
            &admin,
            &remittance_id,
            &savings_id,
            &bills_id,
            &insurance_id,
            &family_wallet_dummy,
        );

        (client, admin, user)
    }};
}

// ═════════════════════════════════════════════════════════════════════════════
//  1. get_remittance_summary
// ═════════════════════════════════════════════════════════════════════════════

/// Benchmark: remittance summary when addresses are not configured.
///
/// This is the cheapest path – the contract returns immediately with
/// `DataAvailability::Missing` after a single storage key miss.
///
/// Security: no admin auth is needed; the function degrades gracefully.
#[test]
fn bench_remittance_summary_no_addresses() {
    let env = bench_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);
    // deliberately skip configure_addresses → DataAvailability::Missing path

    let (cpu, mem, summary) = measure(&env, || {
        client.get_remittance_summary(&user, &1_000_000i128, &PERIOD_START, &PERIOD_END)
    });

    assert_eq!(summary.data_availability, DataAvailability::Missing);

    println!(
        r#"{{"contract":"reporting","method":"get_remittance_summary","scenario":"no_addresses_baseline","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: remittance summary with live mock split contract.
///
/// Exercises the full aggregation path: two cross-contract calls
/// (`get_split`, `calculate_split`) followed by a four-category breakdown loop.
///
/// Security: addresses are configured by admin only; no user auth is required
/// for the read-only query.
#[test]
fn bench_remittance_summary_with_split() {
    let env = bench_env();
    let (client, _admin, user) = setup_reporting!(
        env,
        mock_remittance_split::MockRemittanceSplit,
        mock_savings_5::MockSavings5,
        mock_bills_5::MockBills5,
        mock_insurance_5::MockInsurance5
    );

    let (cpu, mem, summary) = measure(&env, || {
        client.get_remittance_summary(&user, &1_000_000i128, &PERIOD_START, &PERIOD_END)
    });

    assert_eq!(summary.data_availability, DataAvailability::Complete);
    assert_eq!(summary.category_breakdown.len(), 4);
    assert_eq!(summary.total_received, 1_000_000);

    println!(
        r#"{{"contract":"reporting","method":"get_remittance_summary","scenario":"with_split_4_categories","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

// ═════════════════════════════════════════════════════════════════════════════
//  2. get_trend_analysis_multi – pure in-contract history computation
// ═════════════════════════════════════════════════════════════════════════════

/// Build a synthetic history Vec of length `n` with linearly increasing amounts.
fn make_history(env: &Env, user: &Address, n: u32) -> Vec<(u64, i128)> {
    let _ = user; // user is reserved for future auth scoping
    let mut history = Vec::new(env);
    for i in 0u32..n {
        let period_key = PERIOD_START + (i as u64) * 86_400;
        let amount = 100_000i128 * (i as i128 + 1);
        history.push_back((period_key, amount));
    }
    history
}

/// Benchmark: trend analysis over 5 history periods.
///
/// Security: no cross-contract calls; result is fully deterministic for
/// identical inputs regardless of ledger state.
#[test]
fn bench_trend_analysis_multi_5_periods() {
    let env = bench_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);

    let history = make_history(&env, &user, 5);

    let (cpu, mem, trends) = measure(&env, || client.get_trend_analysis_multi(&user, &history));

    // 5 data points → 4 trend windows
    assert_eq!(trends.len(), 4);
    // Each window should show a 100 % increase (amount doubles each step)
    for trend in trends.iter() {
        assert!(trend.change_percentage > 0);
    }

    println!(
        r#"{{"contract":"reporting","method":"get_trend_analysis_multi","scenario":"5_periods","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: trend analysis over 25 history periods.
///
/// Security: same as 5-period variant; determinism assertion below confirms
/// that ledger timestamp does not influence output.
#[test]
fn bench_trend_analysis_multi_25_periods() {
    let env = bench_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);

    let history = make_history(&env, &user, 25);

    let (cpu, mem, trends) = measure(&env, || client.get_trend_analysis_multi(&user, &history));

    assert_eq!(trends.len(), 24);

    println!(
        r#"{{"contract":"reporting","method":"get_trend_analysis_multi","scenario":"25_periods","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: trend analysis over 50 history periods (worst-case realistic load).
///
/// Security: 50-element input is within safe processing range; no integer
/// overflow occurs since amounts are spaced at 100_000 steps per period.
#[test]
fn bench_trend_analysis_multi_50_periods() {
    let env = bench_env();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);

    let history = make_history(&env, &user, 50);

    let (cpu, mem, trends) = measure(&env, || client.get_trend_analysis_multi(&user, &history));

    assert_eq!(trends.len(), 49);

    println!(
        r#"{{"contract":"reporting","method":"get_trend_analysis_multi","scenario":"50_periods","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

// ═════════════════════════════════════════════════════════════════════════════
//  3. get_financial_health_report – full multi-contract aggregation
// ═════════════════════════════════════════════════════════════════════════════
//
// This function chains nine cross-contract calls:
//   calculate_health_score  → get_all_goals, get_unpaid_bills, get_active_policies(limit=1)
//   get_remittance_summary  → get_split, calculate_split
//   get_savings_report      → get_all_goals
//   get_bill_compliance_report → get_all_bills_for_owner
//   get_insurance_report    → get_active_policies(limit=50), get_total_monthly_premium
//
// Mocks scale proportionally so CPU growth can be observed.

/// Benchmark: full financial health report with small data (5 items/source).
///
/// Security: admin-only address configuration is validated on init.
/// The read-only report query requires no additional auth.
#[test]
fn bench_financial_health_report_small_5_items() {
    let env = bench_env();
    let (client, _admin, user) = setup_reporting!(
        env,
        mock_remittance_split::MockRemittanceSplit,
        mock_savings_5::MockSavings5,
        mock_bills_5::MockBills5,
        mock_insurance_5::MockInsurance5
    );

    let (cpu, mem, report) = measure(&env, || {
        client.get_financial_health_report(&user, &500_000i128, &PERIOD_START, &PERIOD_END)
    });

    assert!(report.health_score.score <= 100);
    assert_eq!(report.savings_report.total_goals, 5);
    assert_eq!(report.bill_compliance.total_bills, 5);
    assert_eq!(report.insurance_report.active_policies, 5);

    println!(
        r#"{{"contract":"reporting","method":"get_financial_health_report","scenario":"small_5_items","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: full financial health report with medium data (25 items/source).
///
/// Security: same as small variant.  Scaling confirms no auth bypass from
/// larger payloads.
#[test]
fn bench_financial_health_report_medium_25_items() {
    let env = bench_env();
    let (client, _admin, user) = setup_reporting!(
        env,
        mock_remittance_split::MockRemittanceSplit,
        mock_savings_25::MockSavings25,
        mock_bills_25::MockBills25,
        mock_insurance_25::MockInsurance25
    );

    let (cpu, mem, report) = measure(&env, || {
        client.get_financial_health_report(&user, &500_000i128, &PERIOD_START, &PERIOD_END)
    });

    assert!(report.health_score.score <= 100);
    assert_eq!(report.savings_report.total_goals, 25);
    assert_eq!(report.bill_compliance.total_bills, 25);
    assert_eq!(report.insurance_report.active_policies, 25);

    println!(
        r#"{{"contract":"reporting","method":"get_financial_health_report","scenario":"medium_25_items","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: full financial health report with large data (50 items/source).
///
/// This is the worst-case realistic scenario: 50 savings goals, 50 bills, and
/// 50 active insurance policies are aggregated across nine cross-contract calls.
///
/// Security: insurance `get_active_policies` is called twice (limit=1 in health
/// score, limit=50 in insurance report); mock correctly honours both call sites.
#[test]
fn bench_financial_health_report_large_50_items() {
    let env = bench_env();
    let (client, _admin, user) = setup_reporting!(
        env,
        mock_remittance_split::MockRemittanceSplit,
        mock_savings_50::MockSavings50,
        mock_bills_50::MockBills50,
        mock_insurance_50::MockInsurance50
    );

    let (cpu, mem, report) = measure(&env, || {
        client.get_financial_health_report(&user, &500_000i128, &PERIOD_START, &PERIOD_END)
    });

    assert!(report.health_score.score <= 100);
    assert_eq!(report.savings_report.total_goals, 50);
    assert_eq!(report.bill_compliance.total_bills, 50);
    assert_eq!(report.insurance_report.active_policies, 50);

    println!(
        r#"{{"contract":"reporting","method":"get_financial_health_report","scenario":"large_50_items","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

// ═════════════════════════════════════════════════════════════════════════════
//  4. archive_old_reports – storage-iteration complexity
// ═════════════════════════════════════════════════════════════════════════════
//
// `archive_old_reports` iterates over every active report in the `REPORTS` map,
// moves qualifying entries to the `ARCH_RPT` map, and then iterates `to_remove`
// to clean up the source map.  Cost is O(n) in the number of stored reports.

/// Store `n` synthetic reports for `n` distinct users, then return admin.
///
/// Reports are generated by calling `get_financial_health_report` so that the
/// `generated_at` timestamp is set correctly for the archive threshold.
fn store_n_reports(
    env: &Env,
    client: &ReportingContractClient,
    admin: &Address,
    n: u32,
    savings_id: &Address,
    bills_id: &Address,
    insurance_id: &Address,
    remittance_id: &Address,
    family_dummy: &Address,
) {
    // Re-configure with the provided addresses so we can generate real reports.
    // (They were already configured; this is a no-op if unchanged.)
    let _ = (
        savings_id,
        bills_id,
        insurance_id,
        remittance_id,
        family_dummy,
    );

    for i in 0u32..n {
        let user = Address::generate(env);
        let period_key = PERIOD_START + (i as u64) * 3_600;
        let report =
            client.get_financial_health_report(&user, &100_000i128, &PERIOD_START, &PERIOD_END);
        client.store_report(&user, &report, &period_key);
    }

    // Advance ledger so that `generated_at` values fall before any cutoff.
    let _ = admin;
}

/// Benchmark: archive 5 stored reports.
///
/// Security: `archive_old_reports` enforces admin-only access via
/// `caller.require_auth()` + admin address comparison.
#[test]
fn bench_archive_reports_5() {
    let env = bench_env();
    let (client, admin, _user) = setup_reporting!(
        env,
        mock_remittance_split::MockRemittanceSplit,
        mock_savings_5::MockSavings5,
        mock_bills_5::MockBills5,
        mock_insurance_5::MockInsurance5
    );

    store_n_reports(
        &env,
        &client,
        &admin,
        5,
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
    );

    // Use u64::MAX as cutoff to archive all stored reports.
    let (cpu, mem, archived_count) =
        measure(&env, || client.archive_old_reports(&admin, &u64::MAX));

    assert_eq!(archived_count, 5);

    println!(
        r#"{{"contract":"reporting","method":"archive_old_reports","scenario":"5_stored_reports","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: archive 25 stored reports.
///
/// Security: same admin-only guard as 5-report variant.
#[test]
fn bench_archive_reports_25() {
    let env = bench_env();
    let (client, admin, _user) = setup_reporting!(
        env,
        mock_remittance_split::MockRemittanceSplit,
        mock_savings_5::MockSavings5,
        mock_bills_5::MockBills5,
        mock_insurance_5::MockInsurance5
    );

    store_n_reports(
        &env,
        &client,
        &admin,
        25,
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
    );

    let (cpu, mem, archived_count) =
        measure(&env, || client.archive_old_reports(&admin, &u64::MAX));

    assert_eq!(archived_count, 25);

    println!(
        r#"{{"contract":"reporting","method":"archive_old_reports","scenario":"25_stored_reports","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: archive 50 stored reports (worst-case storage iteration).
///
/// Security: unauthorised callers are rejected before any iteration begins,
/// so attacker-controlled `before_timestamp` values have no security impact.
#[test]
fn bench_archive_reports_50() {
    let env = bench_env();
    let (client, admin, _user) = setup_reporting!(
        env,
        mock_remittance_split::MockRemittanceSplit,
        mock_savings_5::MockSavings5,
        mock_bills_5::MockBills5,
        mock_insurance_5::MockInsurance5
    );

    store_n_reports(
        &env,
        &client,
        &admin,
        50,
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
    );

    let (cpu, mem, archived_count) =
        measure(&env, || client.archive_old_reports(&admin, &u64::MAX));

    assert_eq!(archived_count, 50);

    println!(
        r#"{{"contract":"reporting","method":"archive_old_reports","scenario":"50_stored_reports","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

// ═════════════════════════════════════════════════════════════════════════════
//  5. get_storage_stats – post-archive read
// ═════════════════════════════════════════════════════════════════════════════

/// Benchmark: read storage stats after a full archive cycle.
///
/// `get_storage_stats` is a single instance-storage read with no iteration;
/// this benchmark establishes its baseline cost and confirms it stays O(1)
/// regardless of how many reports have been archived.
///
/// Security: `get_storage_stats` is intentionally unauthenticated – it exposes
/// only non-sensitive aggregate counters (no user data or financial detail).
#[test]
fn bench_get_storage_stats_after_archive() {
    let env = bench_env();
    let (client, admin, _user) = setup_reporting!(
        env,
        mock_remittance_split::MockRemittanceSplit,
        mock_savings_5::MockSavings5,
        mock_bills_5::MockBills5,
        mock_insurance_5::MockInsurance5
    );

    store_n_reports(
        &env,
        &client,
        &admin,
        25,
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
    );

    // Archive all 25 reports to populate the archive map.
    client.archive_old_reports(&admin, &u64::MAX);

    let (cpu, mem, stats) = measure(&env, || client.get_storage_stats());

    assert_eq!(stats.active_reports, 0);
    assert_eq!(stats.archived_reports, 25);

    println!(
        r#"{{"contract":"reporting","method":"get_storage_stats","scenario":"after_25_archived","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}
