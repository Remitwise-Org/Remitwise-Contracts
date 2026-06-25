//! Bound and pagination tests for the archived-reports reader (Issue #832).
//!
//! These tests cover the security/perf fix that bounded the legacy
//! `get_archived_reports` entrypoint to `DEFAULT_PAGE_LIMIT` (20) and made
//! `get_archived_reports_page` use the canonical cursor termination contract
//! (out-of-range cursor returns empty page, `next_cursor == 0`).
//!
//! Behaviour proven here:
//!
//! 1. `get_archived_reports` no longer grows without bound: even with more
//!    archives than `DEFAULT_PAGE_LIMIT`, only the first page is returned.
//! 2. `get_archived_reports_page(0, DEFAULT_PAGE_LIMIT)` returns identical
//!    items to `get_archived_reports` (single source of truth).
//! 3. `get_archived_reports_page` cursor terminates: walking pages from
//!    `cursor=0` reaches `next_cursor=0` after a finite number of calls,
//!    never panics.
//! 4. Out-of-range cursor returns an empty page with `next_cursor == 0`
//!    (canonical terminator) — never panics, never echoes the cursor.
//! 5. Empty archive returns an empty page with `next_cursor == 0`.
//! 6. `limit == 0` is normalized to `DEFAULT_PAGE_LIMIT` (clamp_limit).
//! 7. `limit > MAX_PAGE_LIMIT` is clamped to `MAX_PAGE_LIMIT`.
//! 8. User isolation still holds: the bound cannot be used to read another
//!    user's archive.
//!
//! All tests use a fully configured reporting contract with mocked
//! dependencies and a series of archived reports seeded via the public
//! `archive_old_reports` admin entry (the only path that writes to
//! `ARCH_RPT` / `ARCH_IDX` from outside the contract).

use soroban_sdk::testutils::{Address as AddressTestutils, Address as _, Ledger, LedgerInfo};
use soroban_sdk::{contract, contractimpl, vec, Address, Env};
use testutils::set_ledger_time;

use remitwise_common::{DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};

use crate::{
    Bill, BillComplianceReport, BillPage, BillPaymentsTrait, ContractAddresses, DataAvailability,
    FamilyWalletTrait, FinancialHealthReport, GoalPage, HealthScore, InsurancePolicy,
    InsuranceReport, InsuranceTrait, MemberAddressPage, PolicyPage, RemittanceSplitTrait,
    RemittanceSummary, ReportingContract, ReportingContractClient, SavingsGoal, SavingsGoalsTrait,
    SavingsReport, SpendingPeriod, SpendingTracker,
};

// ============================================================================
// Minimal mock contracts — minimal viable responses so reports build.
// ============================================================================

mod remittance_split_mock {
    use crate::RemittanceSplitTrait;
    use soroban_sdk::{contract, contractimpl, Env, Vec};

    #[contract]
    pub struct RemittanceSplit;

    #[contractimpl]
    impl RemittanceSplitTrait for RemittanceSplit {
        fn get_split(env: &Env) -> Vec<u32> {
            let mut split = Vec::new(env);
            split.push_back(50);
            split.push_back(30);
            split.push_back(15);
            split.push_back(5);
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

mod savings_goals_mock {
    use crate::{GoalPage, SavingsGoal, SavingsGoalsTrait};
    use soroban_sdk::{contract, contractimpl, vec, Address, Env, String as SorobanString, Vec};

    #[contract]
    pub struct SavingsGoals;

    #[contractimpl]
    impl SavingsGoalsTrait for SavingsGoals {
        fn get_all_goals(env: Env, _owner: Address) -> Vec<SavingsGoal> {
            let mut goals = Vec::new(&env);
            goals.push_back(SavingsGoal {
                id: 1,
                owner: <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env),
                name: SorobanString::from_str(&env, "Education"),
                target_amount: 1000,
                current_amount: 500,
                target_date: 1_735_689_600,
                locked: false,
                unlock_date: None,
                tags: vec![&env],
            });
            goals
        }

        fn get_goals(env: Env, _owner: Address, _cursor: u32, _limit: u32) -> GoalPage {
            GoalPage {
                items: vec![&env],
                next_cursor: 0,
                count: 0,
            }
        }

        fn is_goal_completed(_env: Env, _goal_id: u32) -> bool {
            false
        }
    }
}

mod bill_payments_mock {
    use crate::{BillPage, BillPaymentsTrait};
    use soroban_sdk::{contract, contractimpl, vec, Address, Env};

    #[contract]
    pub struct BillPayments;

    #[contractimpl]
    impl BillPaymentsTrait for BillPayments {
        fn get_unpaid_bills(env: Env, _owner: Address, _cursor: u32, _limit: u32) -> BillPage {
            BillPage {
                items: vec![&env],
                next_cursor: 0,
                count: 0,
            }
        }

        fn get_total_unpaid(_env: Env, _owner: Address) -> i128 {
            0
        }

        fn get_all_bills_for_owner(
            env: Env,
            _owner: Address,
            _cursor: u32,
            _limit: u32,
        ) -> BillPage {
            BillPage {
                items: vec![&env],
                next_cursor: 0,
                count: 0,
            }
        }
    }
}

mod insurance_mock {
    use crate::{InsurancePolicy, InsuranceTrait, PolicyPage};
    use soroban_sdk::{contract, contractimpl, vec, Address, Env};

    #[contract]
    pub struct Insurance;

    #[contractimpl]
    impl InsuranceTrait for Insurance {
        fn get_active_policies(env: Env, _owner: Address, _cursor: u32, _limit: u32) -> PolicyPage {
            PolicyPage {
                items: vec![&env],
                next_cursor: 0,
                count: 0,
            }
        }

        fn get_policy(_env: Env, _policy_id: u32) -> Option<InsurancePolicy> {
            None
        }

        fn get_total_monthly_premium(_env: Env, _owner: Address) -> i128 {
            0
        }
    }
}

mod family_wallet_mock {
    use crate::{FamilyWalletTrait, MemberAddressPage, SpendingTracker};
    use soroban_sdk::{contract, contractimpl, vec, Address, Env};

    #[contract]
    pub struct FamilyWallet;

    #[contractimpl]
    impl FamilyWalletTrait for FamilyWallet {
        fn get_owner(env: &Env) -> Address {
            <soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(env)
        }
        fn get_member_addresses_page(env: Env, _cursor: u32, _limit: u32) -> MemberAddressPage {
            MemberAddressPage {
                items: vec![&env],
                next_cursor: 0,
                count: 0,
            }
        }
        fn get_spending_tracker(_env: Env, _member: Address) -> Option<SpendingTracker> {
            None
        }
    }
}

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

/// Build a minimal `FinancialHealthReport` whose `generated_at` is the
/// caller's choice (so seed reports have distinct, ascending timestamps for
/// `archive_old_reports`).
fn make_zero_report(env: &Env, _user: &Address, generated_at: u64) -> FinancialHealthReport {
    FinancialHealthReport {
        health_score: HealthScore {
            score: 0,
            savings_score: 0,
            bills_score: 0,
            insurance_score: 0,
        },
        remittance_summary: RemittanceSummary {
            total_received: 0,
            total_allocated: 0,
            category_breakdown: vec![env],
            period_start: 1_704_067_200,
            period_end: 1_706_745_600,
            data_availability: DataAvailability::Missing,
        },
        savings_report: SavingsReport {
            total_goals: 0,
            completed_goals: 0,
            total_target: 0,
            total_saved: 0,
            completion_percentage: 0,
            period_start: 1_704_067_200,
            period_end: 1_706_745_600,
        },
        bill_compliance: BillComplianceReport {
            total_bills: 0,
            paid_bills: 0,
            unpaid_bills: 0,
            overdue_bills: 0,
            total_amount: 0,
            paid_amount: 0,
            unpaid_amount: 0,
            compliance_percentage: 0,
            period_start: 1_704_067_200,
            period_end: 1_706_745_600,
            data_availability: DataAvailability::Missing,
        },
        insurance_report: InsuranceReport {
            active_policies: 0,
            total_coverage: 0,
            monthly_premium: 0,
            annual_premium: 0,
            coverage_to_premium_ratio: 0,
            period_start: 1_704_067_200,
            period_end: 1_706_745_600,
            data_availability: DataAvailability::Missing,
        },
        data_availability: DataAvailability::Missing,
        generated_at,
    }
}

/// Set up a fully configured reporting contract. Returns `(client, admin)`.
fn setup(env: &Env) -> (ReportingContractClient<'_>, Address) {
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(env, &contract_id);
    let admin = <soroban_sdk::Address as AddressTestutils>::generate(env);
    client.init(&admin);

    let remittance = env.register_contract(None, remittance_split_mock::RemittanceSplit);
    let savings = env.register_contract(None, savings_goals_mock::SavingsGoals);
    let bills = env.register_contract(None, bill_payments_mock::BillPayments);
    let insurance_addr = env.register_contract(None, insurance_mock::Insurance);
    let family_wallet = env.register_contract(None, family_wallet_mock::FamilyWallet);

    client.configure_addresses(
        &admin,
        &remittance,
        &savings,
        &bills,
        &insurance_addr,
        &family_wallet,
    );

    (client, admin)
}

/// Configure a generous ledger TTL so many archive writes fit under
/// `max_entry_ttl`. Required for tests that seed >50 entries.
fn enable_high_ttl(env: &Env) {
    env.ledger().set(LedgerInfo {
        timestamp: 1_704_067_200,
        protocol_version: 20,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 1_700_000,
        max_entry_ttl: 2_000_000,
    });
    set_ledger_time(env, 1, 1_704_067_200);
}

/// Seed `n` archived reports for `user`. Each report is stored under a unique
/// `(user, period_key)` and then archived in one batched admin call.
fn seed_n_archives(
    env: &Env,
    client: &ReportingContractClient,
    admin: &Address,
    user: &Address,
    n: u32,
) {
    enable_high_ttl(env);

    for i in 0..n {
        let generated_at = 1_704_067_200u64 + i as u64;
        let report = make_zero_report(env, user, generated_at);
        client.store_report(user, &report, &(202_400u64 + i as u64));
    }

    let archived = client.archive_old_reports(admin, &u64::MAX);
    assert_eq!(
        archived, n,
        "seed: expected to archive {n} reports, got {archived}"
    );
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn deprecated_get_archived_reports_is_bounded_to_default_page_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let user = <soroban_sdk::Address as AddressTestutils>::generate(&env);

    // Seed `2 * DEFAULT_PAGE_LIMIT + 5` archives to prove the reader does
    // **not** scan the entire index even when there are many more entries.
    let total = (DEFAULT_PAGE_LIMIT * 2) + 5;
    seed_n_archives(&env, &client, &admin, &user, total);

    #[allow(deprecated)]
    let items = client.get_archived_reports(&user);

    assert_eq!(
        items.len(),
        DEFAULT_PAGE_LIMIT,
        "deprecated reader must be capped at DEFAULT_PAGE_LIMIT ({DEFAULT_PAGE_LIMIT}), \
         not grow with the archive size (had {total} entries)",
    );
}

#[test]
fn deprecated_get_archived_reports_matches_first_page_of_paged_reader() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let user = <soroban_sdk::Address as AddressTestutils>::generate(&env);

    let total = DEFAULT_PAGE_LIMIT + 7;
    seed_n_archives(&env, &client, &admin, &user, total);

    #[allow(deprecated)]
    let deprecated = client.get_archived_reports(&user);
    let page = client.get_archived_reports_page(&user, &0u32, &DEFAULT_PAGE_LIMIT);

    assert_eq!(
        deprecated.len(),
        DEFAULT_PAGE_LIMIT,
        "first page must be capped at DEFAULT_PAGE_LIMIT"
    );
    assert_eq!(
        deprecated.len(),
        page.items.len(),
        "deprecated reader and paged-reader first page must return same number of items"
    );

    // Walk the deprecated reader and the paged reader simultaneously and
    // ensure every element matches in order (same period_keys, same scores).
    for i in 0..deprecated.len() {
        let dep_item = deprecated.get(i).expect("deprecated item");
        let page_item = page.items.get(i).expect("page item");
        assert_eq!(
            dep_item.period_key, page_item.period_key,
            "item {i}: deprecated reader and paged-reader first page must agree on period_key"
        );
        assert_eq!(
            dep_item.health_score, page_item.health_score,
            "item {i}: deprecated reader and paged-reader first page must agree on score"
        );
    }

    // Paged-reader metadata should reflect the full archive size.
    assert_eq!(
        page.count, total,
        "paged-reader count must reflect the full archive size"
    );
}

#[test]
fn paged_reader_walks_entire_archive_and_terminates() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let user = <soroban_sdk::Address as AddressTestutils>::generate(&env);

    let page_size = 10u32;
    // 34 entries spans exactly 4 pages of 10 (10 + 10 + 10 + 4).
    let total = page_size * 3 + 4;
    seed_n_archives(&env, &client, &admin, &user, total);

    let mut walked = 0u32;
    let mut cursor = 0u32;
    let mut pages_seen = 0u32;
    loop {
        assert!(
            pages_seen < 100,
            "cursor must terminate within a bounded number of pages (saw {pages_seen})"
        );
        let page = client.get_archived_reports_page(&user, &cursor, &page_size);
        walked = walked.saturating_add(page.items.len());
        pages_seen = pages_seen.saturating_add(1);

        if page.next_cursor == 0 {
            break;
        }
        assert!(
            page.next_cursor > cursor,
            "next_cursor ({}) must be strictly greater than cursor ({}) \
             until termination",
            page.next_cursor,
            cursor
        );
        assert!(
            page.next_cursor <= total,
            "cursor ({}) must never exceed total archive size ({total})",
            page.next_cursor
        );
        cursor = page.next_cursor;
    }

    assert_eq!(
        walked, total,
        "walked all pages: must visit exactly {total} items"
    );
    assert!(
        pages_seen >= 4,
        "expected at least 4 pages for {total} items at size {page_size}"
    );
}

#[test]
fn paged_reader_out_of_range_cursor_returns_empty_page_with_terminator() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let user = <soroban_sdk::Address as AddressTestutils>::generate(&env);

    seed_n_archives(&env, &client, &admin, &user, 5);

    // Cursor past the end.
    let page = client.get_archived_reports_page(&user, &10u32, &5u32);
    assert_eq!(
        page.items.len(),
        0,
        "out-of-range cursor must return an empty page without panicking"
    );
    assert_eq!(
        page.next_cursor, 0,
        "out-of-range cursor must return the canonical terminator (next_cursor == 0), \
         not echo the cursor back"
    );
    assert_eq!(
        page.count, 5,
        "count must still reflect the full archive size"
    );

    // Cursor equal to count is also out-of-range.
    let page = client.get_archived_reports_page(&user, &5u32, &5u32);
    assert_eq!(page.items.len(), 0);
    assert_eq!(page.next_cursor, 0, "cursor == count must terminate");
}

#[test]
fn paged_reader_empty_archive_returns_terminator() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let user = <soroban_sdk::Address as AddressTestutils>::generate(&env);

    let page = client.get_archived_reports_page(&user, &0u32, &DEFAULT_PAGE_LIMIT);
    assert_eq!(page.items.len(), 0, "empty archive returns no items");
    assert_eq!(
        page.next_cursor, 0,
        "empty archive must return the canonical terminator"
    );
    assert_eq!(page.count, 0);
}

#[test]
fn paged_reader_normalizes_zero_limit_to_default_page_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let user = <soroban_sdk::Address as AddressTestutils>::generate(&env);

    seed_n_archives(&env, &client, &admin, &user, DEFAULT_PAGE_LIMIT + 5);

    let page = client.get_archived_reports_page(&user, &0u32, &0u32);
    assert_eq!(
        page.items.len(),
        DEFAULT_PAGE_LIMIT,
        "limit=0 must normalize to DEFAULT_PAGE_LIMIT via clamp_limit"
    );
    assert_eq!(page.next_cursor, DEFAULT_PAGE_LIMIT, "more pages remain");
}

#[test]
fn paged_reader_clamps_oversized_limit_to_max_page_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let user = <soroban_sdk::Address as AddressTestutils>::generate(&env);

    // Seed enough reports to know the clamp didn't shrink the page.
    seed_n_archives(&env, &client, &admin, &user, MAX_PAGE_LIMIT + 5);

    let huge_limit: u32 = u32::MAX;
    let page = client.get_archived_reports_page(&user, &0u32, &huge_limit);
    assert_eq!(
        page.items.len(),
        MAX_PAGE_LIMIT,
        "limit=u32::MAX must clamp to MAX_PAGE_LIMIT ({MAX_PAGE_LIMIT}) via clamp_limit"
    );
    assert_eq!(
        page.next_cursor, MAX_PAGE_LIMIT,
        "more pages remain after the clamped first page"
    );
}

#[test]
fn paged_reader_user_isolation_holds_under_bound() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let user_a = <soroban_sdk::Address as AddressTestutils>::generate(&env);
    let user_b = <soroban_sdk::Address as AddressTestutils>::generate(&env);

    seed_n_archives(&env, &client, &admin, &user_a, DEFAULT_PAGE_LIMIT + 10);
    seed_n_archives(&env, &client, &admin, &user_b, 3);

    // user_a sees only their own archive (still capped, never panic).
    let page_a = client.get_archived_reports_page(&user_a, &0u32, &DEFAULT_PAGE_LIMIT);
    assert_eq!(page_a.items.len(), DEFAULT_PAGE_LIMIT);
    for r in page_a.items.iter() {
        assert_eq!(r.user, user_a, "user_a must only see their own archive");
    }

    // user_b sees only their own (smaller) archive.
    let page_b = client.get_archived_reports_page(&user_b, &0u32, &DEFAULT_PAGE_LIMIT);
    assert_eq!(page_b.items.len(), 3);
    for r in page_b.items.iter() {
        assert_eq!(r.user, user_b, "user_b must only see their own archive");
    }

    // Walking user_a's archive never exposes user_b's stored rows.
    let mut cursor = 0u32;
    let mut walked_a = 0u32;
    loop {
        let page = client.get_archived_reports_page(&user_a, &cursor, &10u32);
        walked_a = walked_a.saturating_add(page.items.len());
        for r in page.items.iter() {
            assert_ne!(
                r.user, user_b,
                "user_a's archive must never contain user_b's rows"
            );
        }
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }
    assert_eq!(
        walked_a,
        DEFAULT_PAGE_LIMIT + 10,
        "user_a's full archive walk returns the seeded count"
    );
}
