#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
use soroban_sdk::{symbol_short, Address, Env, Map, Vec};

use crate::{
    BillComplianceReport, DataAvailability, FinancialHealthReport, HealthScore, InsuranceReport,
    RemittanceSummary, ReportScope, ReportingContract, ReportingContractClient, ReportingError,
    SavingsReport,
};

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
}

fn make_test_report(env: &Env, generated_at: u64) -> FinancialHealthReport {
    FinancialHealthReport {
        health_score: HealthScore {
            score: 75,
            savings_score: 30,
            bills_score: 30,
            insurance_score: 15,
        },
        remittance_summary: RemittanceSummary {
            total_received: 1000,
            total_allocated: 1000,
            category_breakdown: Vec::new(env),
            period_start: 1_700_000_000,
            period_end: 1_700_100_000,
            data_availability: DataAvailability::Complete,
        },
        savings_report: SavingsReport {
            total_goals: 1,
            completed_goals: 1,
            total_target: 500,
            total_saved: 500,
            completion_percentage: 100,
            period_start: 1_700_000_000,
            period_end: 1_700_100_000,
        },
        bill_compliance: BillComplianceReport {
            total_bills: 2,
            paid_bills: 2,
            unpaid_bills: 0,
            overdue_bills: 0,
            total_amount: 200,
            paid_amount: 200,
            unpaid_amount: 0,
            compliance_percentage: 100,
            period_start: 1_700_000_000,
            period_end: 1_700_100_000,
            data_availability: DataAvailability::Complete,
        },
        insurance_report: InsuranceReport {
            active_policies: 1,
            total_coverage: 5000,
            monthly_premium: 50,
            annual_premium: 600,
            coverage_to_premium_ratio: 100,
            period_start: 1_700_000_000,
            period_end: 1_700_100_000,
            data_availability: DataAvailability::Complete,
        },
        data_availability: DataAvailability::Complete,
        generated_at,
    }
}

/// Reproduction steps 1-4:
/// 1. init and store a report for (user, 202401) with generated_at = 100.
/// 2. Admin archive_old_reports(before = 200) -> 1 archived.
/// 3. Store a report for (user, 202401) again with generated_at = 150. Admin archives again -> 1 archived.
/// 4. get_archived_reports_page(user, 0, 10) -> count == 1 and exactly one item.
#[test]
fn test_rearchive_same_period_keeps_single_index_entry() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);
    let user = Address::generate(&env);

    // Step 1: store a report for (user, 202401) with generated_at = 100.
    let report1 = make_test_report(&env, 100);
    assert!(client.store_report(&user, &report1, &202401));

    // Step 2: Admin archive_old_reports(before = 200) -> 1 archived.
    let n1 = client.archive_old_reports(&admin, &200);
    assert_eq!(n1, 1);

    // Step 3: Store a report for (user, 202401) again with generated_at = 150. Admin archives again -> 1 archived.
    let report2 = make_test_report(&env, 150);
    assert!(client.store_report(&user, &report2, &202401));
    let n2 = client.archive_old_reports(&admin, &200);
    assert_eq!(n2, 1);

    // Step 4: get_archived_reports_page(user, 0, 10).
    // Expected: count == 1, exactly one item with updated report (generated_at == 150).
    let page = client.get_archived_reports_page(&user, &0, &10);
    assert_eq!(page.count, 1, "count must equal 1, not 2");
    assert_eq!(page.items.len(), 1, "must have exactly one item");
    let item = page.items.get(0).unwrap();
    assert_eq!(item.period_key, 202401);
    assert_eq!(item.generated_at, 150);

    // StorageStats match the true number of archived records
    let stats = client.get_storage_stats();
    assert_eq!(stats.archived_reports, 1);
    assert_eq!(stats.active_reports, 0);
}

/// Test cleanup_removes_all_occurrences: inject a duplicated index via env.as_contract,
/// run cleanup, and count == 0 and the user key is absent from ARCH_IDX.
#[test]
fn test_cleanup_removes_all_occurrences() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);
    let user = Address::generate(&env);

    let period_key = 202401u64;
    let report = make_test_report(&env, 100);
    client.store_report(&user, &report, &period_key);
    client.archive_old_reports(&admin, &200);

    // Inject a duplicated index via env.as_contract
    env.as_contract(&contract_id, || {
        let mut arch_idx: Map<Address, Vec<u64>> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_IDX"))
            .unwrap_or_else(|| Map::new(&env));
        let mut user_idx = Vec::new(&env);
        user_idx.push_back(period_key);
        user_idx.push_back(period_key);
        arch_idx.set(user.clone(), user_idx);
        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_IDX"), &arch_idx);
    });

    // Run cleanup
    let deleted = client.cleanup_old_reports(&admin, &u64::MAX);
    assert_eq!(deleted, 1);

    // Count == 0 and page items empty
    let page = client.get_archived_reports_page(&user, &0, &10);
    assert_eq!(page.count, 0);
    assert_eq!(page.items.len(), 0);

    // User key is absent from ARCH_IDX
    env.as_contract(&contract_id, || {
        let arch_idx: Map<Address, Vec<u64>> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_IDX"))
            .unwrap_or_else(|| Map::new(&env));
        assert!(
            arch_idx.get(user.clone()).is_none(),
            "user key must be absent from ARCH_IDX"
        );
    });
}

/// Test page_count_matches_items_across_pages: 45 distinct archived periods,
/// limit = 20 -> pages of 20/20/5, count == 45 on each page, cursors 20, 40, 0, no duplicates.
#[test]
fn test_page_count_matches_items_across_pages() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    enable_high_ttl(&env);
    env.budget().reset_unlimited();

    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);
    let user = Address::generate(&env);

    // 45 distinct archived periods
    let total_periods = 45u32;
    for i in 0..total_periods {
        let period_key = 202400u64 + i as u64;
        let report = make_test_report(&env, 100 + i as u64);
        client.store_report(&user, &report, &period_key);
    }

    let archived = client.archive_old_reports(&admin, &u64::MAX);
    assert_eq!(archived, total_periods);

    let limit = 20u32;

    // Page 1: cursor = 0, limit = 20
    let page1 = client.get_archived_reports_page(&user, &0, &limit);
    assert_eq!(page1.items.len(), 20);
    assert_eq!(page1.count, 45);
    assert_eq!(page1.next_cursor, 20);

    // Page 2: cursor = 20, limit = 20
    let page2 = client.get_archived_reports_page(&user, &20, &limit);
    assert_eq!(page2.items.len(), 20);
    assert_eq!(page2.count, 45);
    assert_eq!(page2.next_cursor, 40);

    // Page 3: cursor = 40, limit = 20
    let page3 = client.get_archived_reports_page(&user, &40, &limit);
    assert_eq!(page3.items.len(), 5);
    assert_eq!(page3.count, 45);
    assert_eq!(page3.next_cursor, 0);

    // Verify all 45 items are distinct (no duplicates across pages)
    let mut all_keys: std::vec::Vec<u64> = std::vec::Vec::new();
    for item in page1.items.iter() {
        assert!(!all_keys.contains(&item.period_key));
        all_keys.push(item.period_key);
    }
    for item in page2.items.iter() {
        assert!(!all_keys.contains(&item.period_key));
        all_keys.push(item.period_key);
    }
    for item in page3.items.iter() {
        assert!(!all_keys.contains(&item.period_key));
        all_keys.push(item.period_key);
    }
    assert_eq!(all_keys.len(), 45);
}

/// Test repair_archive_index_rebuilds_from_records: corrupted index
/// [k1, k1, k2, k_missing] -> after repair [k1, k2] (sorted), count == 2.
/// A non-admin call -> Unauthorized.
#[test]
fn test_repair_archive_index_rebuilds_from_records() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);
    let user = Address::generate(&env);
    let attacker = Address::generate(&env);

    let k1 = 202401u64;
    let k2 = 202402u64;
    let k_missing = 202499u64;

    let r1 = make_test_report(&env, 100);
    let r2 = make_test_report(&env, 110);
    client.store_report(&user, &r1, &k1);
    client.store_report(&user, &r2, &k2);
    let archived = client.archive_old_reports(&admin, &u64::MAX);
    assert_eq!(archived, 2);

    // Inject corrupted index: [k1, k1, k2, k_missing]
    env.as_contract(&contract_id, || {
        let mut arch_idx: Map<Address, Vec<u64>> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_IDX"))
            .unwrap_or_else(|| Map::new(&env));
        let mut corrupted = Vec::new(&env);
        corrupted.push_back(k1);
        corrupted.push_back(k1);
        corrupted.push_back(k2);
        corrupted.push_back(k_missing);
        arch_idx.set(user.clone(), corrupted);
        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_IDX"), &arch_idx);
    });

    // A non-admin call -> Unauthorized
    let err = client.try_repair_archive_index(&attacker, &user);
    assert_eq!(err, Err(Ok(ReportingError::Unauthorized)));

    // Admin call -> repairs successfully
    let repaired_count = client.repair_archive_index(&admin, &user);
    assert_eq!(repaired_count, 2);

    // Verify index after repair is [k1, k2] (sorted), count == 2
    let page = client.get_archived_reports_page(&user, &0, &10);
    assert_eq!(page.count, 2);
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items.get(0).unwrap().period_key, k1);
    assert_eq!(page.items.get(1).unwrap().period_key, k2);
    assert!(k1 < k2);
}

/// Test delegated_page_matches_owner_page: get_archived_reports_page_for with
/// a valid viewer grant returns the same items and count as the owner call in
/// the scenario above.
#[test]
fn test_delegated_page_matches_owner_page() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);
    let owner = Address::generate(&env);
    let viewer = Address::generate(&env);

    let k1 = 202401u64;
    let k2 = 202402u64;

    let r1 = make_test_report(&env, 100);
    let r2 = make_test_report(&env, 110);
    client.store_report(&owner, &r1, &k1);
    client.store_report(&owner, &r2, &k2);
    client.archive_old_reports(&admin, &u64::MAX);

    // Grant viewer Archived scope
    client.grant_viewer(&owner, &viewer, &ReportScope::Archived, &0);

    let owner_page = client.get_archived_reports_page(&owner, &0, &10);
    let delegated_page = client.get_archived_reports_page_for(&viewer, &owner, &0, &10);

    assert_eq!(delegated_page.count, owner_page.count);
    assert_eq!(delegated_page.next_cursor, owner_page.next_cursor);
    assert_eq!(delegated_page.items.len(), owner_page.items.len());
    for i in 0..owner_page.items.len() {
        let owner_item = owner_page.items.get(i).unwrap();
        let delegated_item = delegated_page.items.get(i).unwrap();
        assert_eq!(delegated_item.period_key, owner_item.period_key);
        assert_eq!(delegated_item.health_score, owner_item.health_score);
        assert_eq!(delegated_item.generated_at, owner_item.generated_at);
        assert_eq!(delegated_item.archived_at, owner_item.archived_at);
        assert_eq!(delegated_item.user, owner_item.user);
    }
}
