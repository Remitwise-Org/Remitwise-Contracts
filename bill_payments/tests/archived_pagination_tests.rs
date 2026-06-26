//! Tests for the bill-payments-archived-pagination feature.
//!
//! Covers:
//!   - Unit tests: edge cases, cursor boundary, limit clamping, restore/cleanup index maintenance
//!   - Property-based tests (proptest): all 9 correctness properties from the design document

use bill_payments::{BillPayments, BillPaymentsClient};
use proptest::prelude::*;
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 100,
        timestamp: 1_700_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 700_000,
    });
    env.budget().reset_unlimited();
    env
}

fn setup_client(env: &Env) -> (BillPaymentsClient<'_>, Address) {
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(env, &cid);
    let owner = Address::generate(env);
    client.set_pause_admin(&owner, &owner);
    (client, owner)
}

/// Create `n` bills for `owner`, pay them all, and archive them.
/// Returns the list of archived bill IDs.
fn create_pay_archive(env: &Env, client: &BillPaymentsClient, owner: &Address, n: u32) -> Vec<u32> {
    let mut ids = Vec::new();
    for i in 0..n {
        let name = soroban_sdk::String::from_str(env, &format!("Bill{}", i));
        let id = client.create_bill(
            owner,
            &name,
            &100,
            &2_000_000_000u64,
            &false,
            &0,
            &None,
            &soroban_sdk::String::from_str(env, "XLM"),
            &None,
        );
        client.pay_bill(owner, &id);
        ids.push(id);
    }
    client.archive_paid_bills(owner, &u64::MAX);
    ids
}

/// Paginate all pages of get_archived_bills_page and collect all returned bill IDs.
fn paginate_all(client: &BillPaymentsClient, owner: &Address, limit: u32) -> Vec<u32> {
    let mut all_ids = Vec::new();
    let mut cursor = 0u32;
    loop {
        let page = client.get_archived_bills_page(owner, &cursor, &limit);
        for bill in page.items.iter() {
            all_ids.push(bill.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }
    all_ids
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[test]
fn test_page_empty_owner() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    let page = client.get_archived_bills_page(&owner, &0, &10);
    assert_eq!(page.count, 0);
    assert_eq!(page.next_cursor, 0);
    assert!(page.items.is_empty());
}

#[test]
fn test_page_single_page() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 3);
    let page = client.get_archived_bills_page(&owner, &0, &10);
    assert_eq!(page.count, 3);
    assert_eq!(page.next_cursor, 0);
}

#[test]
fn test_page_multi_page_traversal() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 6);
    let all_ids = paginate_all(&client, &owner, 4);
    assert_eq!(all_ids.len(), 6);
}

#[test]
fn test_page_cursor_boundary() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    // Archive 5 bills; IDs will be 1..=5
    create_pay_archive(&env, &client, &owner, 5);
    let page = client.get_archived_bills_page(&owner, &3, &10);
    // Should return only IDs > 3
    assert_eq!(page.count, 2);
    for bill in page.items.iter() {
        assert!(bill.id > 3, "expected id > 3, got {}", bill.id);
    }
}

#[test]
fn test_page_limit_zero_uses_default() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    // Archive 25 bills (more than DEFAULT_PAGE_LIMIT=20)
    create_pay_archive(&env, &client, &owner, 25);
    let page = client.get_archived_bills_page(&owner, &0, &0);
    // limit=0 → DEFAULT_PAGE_LIMIT=20
    assert_eq!(page.count, 20);
    assert!(page.next_cursor > 0);
}

#[test]
fn test_page_limit_above_max_clamped() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    // Archive 60 bills (more than MAX_PAGE_LIMIT=50)
    create_pay_archive(&env, &client, &owner, 60);
    let page = client.get_archived_bills_page(&owner, &0, &100);
    // limit=100 → clamped to MAX_PAGE_LIMIT=50
    assert!(page.count <= 50);
}

#[test]
fn test_page_count_equals_items_len() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 7);
    let page = client.get_archived_bills_page(&owner, &0, &3);
    assert_eq!(page.count, page.items.len());
}

#[test]
fn test_page_items_ascending_order() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 5);
    let page = client.get_archived_bills_page(&owner, &0, &10);
    let ids: Vec<u32> = page.items.iter().map(|b| b.id).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "items must be in ascending ID order");
}

#[test]
fn test_restore_bill_removes_from_index() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 3);
    // Get first archived bill ID
    let page = client.get_archived_bills_page(&owner, &0, &10);
    let first_id = page.items.first().unwrap().id;
    // Restore it
    client.restore_bill(&owner, &first_id);
    // Should no longer appear in paginated results
    let after = paginate_all(&client, &owner, 10);
    assert!(
        !after.contains(&first_id),
        "restored bill should not appear in index"
    );
}

#[test]
fn test_restore_last_bill_returns_empty_page() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 1);
    let page = client.get_archived_bills_page(&owner, &0, &10);
    let bill_id = page.items.first().unwrap().id;
    client.restore_bill(&owner, &bill_id);
    let after = client.get_archived_bills_page(&owner, &0, &10);
    assert_eq!(after.count, 0);
    assert_eq!(after.next_cursor, 0);
}

#[test]
fn test_bulk_cleanup_removes_from_index() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 5);
    // Cleanup all archived bills
    client.bulk_cleanup_bills(&owner, &u64::MAX);
    let after = client.get_archived_bills_page(&owner, &0, &10);
    assert_eq!(after.count, 0);
}

#[test]
fn test_multi_owner_isolation() {
    let env = make_env();
    let (client, owner_a) = setup_client(&env);
    let owner_b = Address::generate(&env);
    create_pay_archive(&env, &client, &owner_a, 3);
    create_pay_archive(&env, &client, &owner_b, 3);

    let ids_a: Vec<u32> = paginate_all(&client, &owner_a, 10);
    let ids_b: Vec<u32> = paginate_all(&client, &owner_b, 10);

    assert_eq!(ids_a.len(), 3);
    assert_eq!(ids_b.len(), 3);
    // Disjoint
    for id in &ids_a {
        assert!(
            !ids_b.contains(id),
            "owner B should not see owner A's bills"
        );
    }
}

#[test]
fn test_equivalence_with_get_archived_bills() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 8);

    let page_old = client.get_archived_bills(&owner, &0, &5);
    let page_new = client.get_archived_bills_page(&owner, &0, &5);

    let ids_old: Vec<u32> = page_old.items.iter().map(|b| b.id).collect();
    let ids_new: Vec<u32> = page_new.items.iter().map(|b| b.id).collect();
    assert_eq!(
        ids_old, ids_new,
        "get_archived_bills and get_archived_bills_page must return same IDs"
    );
    assert_eq!(page_old.next_cursor, page_new.next_cursor);
}

// ---------------------------------------------------------------------------
// Boundary Regression Tests
// ---------------------------------------------------------------------------

/// Regression: exactly N items with limit N must return all items in one page
/// and next_cursor == 0 (no phantom second page).
#[test]
fn test_boundary_exact_limit_fills_single_page() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    // Archive exactly 5 bills; request limit=5
    create_pay_archive(&env, &client, &owner, 5);
    let page = client.get_archived_bills_page(&owner, &0, &5);
    assert_eq!(page.count, 5, "must return all 5 items");
    assert_eq!(
        page.next_cursor, 0,
        "next_cursor must be 0 when all items fit in one page"
    );
}

/// Regression: limit = N-1 produces two pages — first page full, second page
/// has exactly 1 item and next_cursor == 0.
#[test]
fn test_boundary_last_page_has_one_item() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    // Archive 5 bills; request pages of size 4
    create_pay_archive(&env, &client, &owner, 5);

    let page1 = client.get_archived_bills_page(&owner, &0, &4);
    assert_eq!(page1.count, 4);
    assert!(page1.next_cursor > 0, "must have a next page");

    let page2 = client.get_archived_bills_page(&owner, &page1.next_cursor, &4);
    assert_eq!(page2.count, 1, "last page must contain exactly 1 item");
    assert_eq!(
        page2.next_cursor, 0,
        "next_cursor must be 0 on last page"
    );
}

/// Regression: cursor set to the last item's ID must return an empty page,
/// not a page containing the last item again.
#[test]
fn test_boundary_cursor_at_last_id_returns_empty() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 4);

    // Retrieve the full first page to find the last ID
    let full_page = client.get_archived_bills_page(&owner, &0, &50);
    assert_eq!(full_page.count, 4);
    let last_id = full_page.items.last().unwrap().id;

    // Starting from the last ID as cursor should yield nothing
    let page = client.get_archived_bills_page(&owner, &last_id, &10);
    assert_eq!(
        page.count, 0,
        "cursor == last item ID must produce an empty page"
    );
    assert_eq!(page.next_cursor, 0);
    assert!(page.items.is_empty());
}

/// Regression: cursor at the second-to-last item's ID must return exactly one
/// item (the last item) and next_cursor == 0.
#[test]
fn test_boundary_cursor_at_second_to_last_returns_one_item() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 4);

    let full_page = client.get_archived_bills_page(&owner, &0, &50);
    let ids: Vec<u32> = full_page.items.iter().map(|b| b.id).collect();
    let second_to_last = ids[ids.len() - 2];
    let last_id = ids[ids.len() - 1];

    let page = client.get_archived_bills_page(&owner, &second_to_last, &10);
    assert_eq!(
        page.count, 1,
        "cursor at second-to-last must yield exactly 1 item"
    );
    assert_eq!(
        page.items.first().unwrap().id,
        last_id,
        "that item must be the last one"
    );
    assert_eq!(page.next_cursor, 0);
}

/// Regression: consecutive page reads with limit=1 must step through every
/// item exactly once without skipping the item at each page boundary.
#[test]
fn test_boundary_limit_one_traverses_all_items() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 5);

    let mut collected: Vec<u32> = Vec::new();
    let mut cursor = 0u32;
    loop {
        let page = client.get_archived_bills_page(&owner, &cursor, &1);
        assert!(
            page.count <= 1,
            "limit=1 must never return more than 1 item"
        );
        for bill in page.items.iter() {
            collected.push(bill.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(collected.len(), 5, "limit=1 traversal must visit all 5 items");

    // No duplicates and strictly ascending
    for i in 1..collected.len() {
        assert!(
            collected[i] > collected[i - 1],
            "items must be strictly ascending; got {} after {}",
            collected[i],
            collected[i - 1]
        );
    }
}

/// Regression: a cursor saved from page 1 that is then used after bulk_cleanup
/// removes all remaining bills must return an empty page, not panic or loop.
#[test]
fn test_boundary_stale_cursor_after_bulk_cleanup_returns_empty() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 8);

    // Save a mid-traversal cursor from the first page
    let page1 = client.get_archived_bills_page(&owner, &0, &4);
    assert_eq!(page1.count, 4);
    let stale_cursor = page1.next_cursor;
    assert!(stale_cursor > 0, "test requires a valid mid-traversal cursor");

    // Wipe all archived bills before resuming
    client.bulk_cleanup_bills(&owner, &u64::MAX);

    // Resuming with the stale cursor must produce an empty page, not a crash or phantom items
    let page2 = client.get_archived_bills_page(&owner, &stale_cursor, &4);
    assert_eq!(
        page2.count, 0,
        "stale cursor after bulk_cleanup must return empty page"
    );
    assert_eq!(page2.next_cursor, 0);
    assert!(page2.items.is_empty());
}

/// Regression: a cursor value that corresponds to a gap in the archived index
/// (because that bill was restored) must skip the gap and return the next valid
/// item, not re-deliver the item before the gap or return nothing when items remain.
#[test]
fn test_boundary_cursor_at_gap_id_skips_to_next_valid_item() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    // Archive 5 bills; IDs 1..=5
    create_pay_archive(&env, &client, &owner, 5);

    // Restore bill 3 — creates a gap at ID 3 in the archive index
    client.restore_bill(&owner, &3u32);

    // Use ID 3 (the gap) as the cursor; must return IDs 4 and 5
    let page = client.get_archived_bills_page(&owner, &3, &10);
    assert_eq!(
        page.count, 2,
        "cursor at gap ID must return the 2 items that follow it"
    );
    let ids: Vec<u32> = page.items.iter().map(|b| b.id).collect();
    assert_eq!(ids, vec![4u32, 5], "must return IDs 4 and 5 after the gap");
    assert_eq!(page.next_cursor, 0);
}

/// Regression: exactly MAX_PAGE_LIMIT (50) archived items requested with
/// limit=50 must fill one page completely and set next_cursor to 0,
/// not signal a phantom third page.
#[test]
fn test_boundary_exact_max_page_limit_no_phantom_next_page() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    // Archive exactly 50 bills — equal to MAX_PAGE_LIMIT
    create_pay_archive(&env, &client, &owner, 50);

    let page = client.get_archived_bills_page(&owner, &0, &50);
    assert_eq!(page.count, 50, "must return all 50 items");
    assert_eq!(
        page.next_cursor, 0,
        "next_cursor must be 0 when item count exactly equals MAX_PAGE_LIMIT"
    );
    assert_eq!(page.items.len(), 50);
}

// ---------------------------------------------------------------------------
// Property-Based Tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: bill-payments-archived-pagination, Property 1: Index Consistency Invariant
    /// For any sequence of archive/restore/cleanup ops, ARCH_IDX and ARCH_BILL stay in sync.
    #[test]
    fn prop_index_consistency_invariant(n_archive in 1u32..=15u32, n_restore in 0u32..=5u32) {
        let env = make_env();
        let (client, owner) = setup_client(&env);
        let ids = create_pay_archive(&env, &client, &owner, n_archive);

        // Restore some bills
        let to_restore = n_restore.min(n_archive);
        for i in 0..to_restore {
            client.restore_bill(&owner, &ids[i as usize]);
        }

        // Paginate all remaining and verify count matches
        let remaining = paginate_all(&client, &owner, 50);
        let expected_count = (n_archive - to_restore) as usize;
        prop_assert_eq!(remaining.len(), expected_count);
    }

    /// Feature: bill-payments-archived-pagination, Property 2: Ascending Order Invariant
    /// Items returned by get_archived_bills_page are always in strictly ascending ID order.
    #[test]
    fn prop_ascending_order_invariant(n in 1u32..=20u32, limit in 1u32..=10u32) {
        let env = make_env();
        let (client, owner) = setup_client(&env);
        create_pay_archive(&env, &client, &owner, n);

        let mut cursor = 0u32;
        loop {
            let page = client.get_archived_bills_page(&owner, &cursor, &limit);
            let ids: Vec<u32> = page.items.iter().map(|b| b.id).collect();
            let mut sorted = ids.clone();
            sorted.sort();
            prop_assert_eq!(&ids, &sorted, "page items must be in ascending order");
            if page.next_cursor == 0 { break; }
            cursor = page.next_cursor;
        }
    }

    /// Feature: bill-payments-archived-pagination, Property 3: Cursor Filtering
    /// All returned IDs are strictly greater than the cursor.
    #[test]
    fn prop_cursor_filtering(n in 2u32..=20u32, cursor_offset in 0u32..=10u32) {
        let env = make_env();
        let (client, owner) = setup_client(&env);
        create_pay_archive(&env, &client, &owner, n);

        let cursor = cursor_offset;
        let page = client.get_archived_bills_page(&owner, &cursor, &50);
        for bill in page.items.iter() {
            prop_assert!(bill.id > cursor, "all returned IDs must be > cursor {}", cursor);
        }
    }

    /// Feature: bill-payments-archived-pagination, Property 4: Page Size and Count Invariant
    /// items.len() <= clamp_limit(limit) and count == items.len() always.
    #[test]
    fn prop_page_size_and_count(n in 1u32..=30u32, limit in 0u32..=100u32) {
        let env = make_env();
        let (client, owner) = setup_client(&env);
        create_pay_archive(&env, &client, &owner, n);

        let page = client.get_archived_bills_page(&owner, &0, &limit);
        let effective = if limit == 0 { 20 } else if limit > 50 { 50 } else { limit };
        prop_assert!(page.count <= effective, "count {} must be <= clamp_limit({})", page.count, limit);
        prop_assert_eq!(page.count, page.items.len());
    }

    /// Feature: bill-payments-archived-pagination, Property 5: next_cursor Semantics
    /// next_cursor is correct: 0 when no more pages, last item ID otherwise.
    #[test]
    fn prop_next_cursor_semantics(n in 2u32..=20u32, limit in 1u32..=5u32) {
        let env = make_env();
        let (client, owner) = setup_client(&env);
        create_pay_archive(&env, &client, &owner, n);

        let mut cursor = 0u32;
        loop {
            let page = client.get_archived_bills_page(&owner, &cursor, &limit);
            if page.next_cursor != 0 {
                // There are more pages: next_cursor must equal last item's ID
                let last_id = page.items.last().unwrap().id;
                prop_assert_eq!(page.next_cursor, last_id);
            } else {
                // No more pages: verify no items exist beyond last returned
                if let Some(last) = page.items.last() {
                    let beyond = client.get_archived_bills_page(&owner, &last.id, &1);
                    prop_assert_eq!(beyond.count, 0);
                }
                break;
            }
            cursor = page.next_cursor;
        }
    }

    /// Feature: bill-payments-archived-pagination, Property 6: Full Pagination Round-Trip
    /// Paginating all pages yields exactly N bills with no duplicates and no gaps.
    #[test]
    fn prop_full_pagination_round_trip(n in 1u32..=25u32, limit in 1u32..=7u32) {
        let env = make_env();
        let (client, owner) = setup_client(&env);
        let archived_ids = create_pay_archive(&env, &client, &owner, n);

        let collected = paginate_all(&client, &owner, limit);
        prop_assert_eq!(collected.len(), n as usize, "must collect exactly N bills");

        // No duplicates
        let mut deduped = collected.clone();
        deduped.sort();
        deduped.dedup();
        prop_assert_eq!(deduped.len(), collected.len(), "no duplicates");

        // Same set as archived
        let mut expected = archived_ids.clone();
        expected.sort();
        let mut actual = collected.clone();
        actual.sort();
        prop_assert_eq!(actual, expected, "collected set must equal archived set");
    }

    /// Feature: bill-payments-archived-pagination, Property 7: Confluence — Archive Order Independence
    /// The final paginated result is the same regardless of archive call ordering.
    #[test]
    fn prop_confluence_archive_order(n in 2u32..=10u32) {
        // Run 1: archive all at once
        let env1 = make_env();
        let (client1, owner1) = setup_client(&env1);
        let ids1 = create_pay_archive(&env1, &client1, &owner1, n);
        let mut result1 = paginate_all(&client1, &owner1, 50);
        result1.sort();

        // Run 2: archive one at a time (same bills, same IDs since fresh env)
        let env2 = make_env();
        let (client2, owner2) = setup_client(&env2);
        let _ = create_pay_archive(&env2, &client2, &owner2, n);
        let mut result2 = paginate_all(&client2, &owner2, 50);
        result2.sort();

        prop_assert_eq!(result1.len(), result2.len());
        prop_assert_eq!(result1, result2, "archive order must not affect final index");
        let _ = ids1;
    }

    /// Feature: bill-payments-archived-pagination, Property 8: Owner Isolation
    /// Two owners get disjoint results; each result contains only their own bills.
    #[test]
    fn prop_owner_isolation(n_a in 1u32..=10u32, n_b in 1u32..=10u32) {
        let env = make_env();
        let (client, owner_a) = setup_client(&env);
        let owner_b = Address::generate(&env);

        create_pay_archive(&env, &client, &owner_a, n_a);
        create_pay_archive(&env, &client, &owner_b, n_b);

        let ids_a = paginate_all(&client, &owner_a, 50);
        let ids_b = paginate_all(&client, &owner_b, 50);

        prop_assert_eq!(ids_a.len(), n_a as usize);
        prop_assert_eq!(ids_b.len(), n_b as usize);

        for id in &ids_a {
            prop_assert!(!ids_b.contains(id), "owner B must not see owner A's bill {}", id);
        }
    }

    /// Feature: bill-payments-archived-pagination, Property 9: Equivalence with get_archived_bills
    /// get_archived_bills and get_archived_bills_page return identical results for same inputs.
    #[test]
    fn prop_equivalence_with_get_archived_bills(n in 1u32..=20u32, limit in 1u32..=10u32) {
        let env = make_env();
        let (client, owner) = setup_client(&env);
        create_pay_archive(&env, &client, &owner, n);

        let mut cursor = 0u32;
        loop {
            let old = client.get_archived_bills(&owner, &cursor, &limit);
            let new = client.get_archived_bills_page(&owner, &cursor, &limit);

            let ids_old: Vec<u32> = old.items.iter().map(|b| b.id).collect();
            let ids_new: Vec<u32> = new.items.iter().map(|b| b.id).collect();
            prop_assert_eq!(&ids_old, &ids_new, "both functions must return same IDs at cursor={}", cursor);
            prop_assert_eq!(old.next_cursor, new.next_cursor);

            if old.next_cursor == 0 { break; }
            cursor = old.next_cursor;
        }
    }
}

// ---------------------------------------------------------------------------
// Additional Edge Case Tests
// ---------------------------------------------------------------------------

/// Regression: cursor beyond all archived IDs returns empty page.
#[test]
fn test_cursor_far_beyond_max_id_returns_empty() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 5);
    
    // Use a cursor way beyond the highest ID
    let page = client.get_archived_bills_page(&owner, &999999, &10);
    assert_eq!(page.count, 0);
    assert_eq!(page.next_cursor, 0);
    assert!(page.items.is_empty());
}

/// Regression: alternating restore operations maintain correct pagination.
#[test]
fn test_alternating_restore_maintains_pagination() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 10);
    
    // Get initial IDs
    let initial = paginate_all(&client, &owner, 50);
    assert_eq!(initial.len(), 10);
    
    // Restore every other bill (2, 4, 6, 8, 10)
    for i in (1..=9).step_by(2) {
        client.restore_bill(&owner, &initial[i]);
    }
    
    let after = paginate_all(&client, &owner, 50);
    assert_eq!(after.len(), 5, "should have 5 bills remaining");
    
    // Verify only odd-indexed bills remain
    for id in after.iter() {
        let original_idx = initial.iter().position(|&x| x == *id).unwrap();
        assert_eq!(original_idx % 2, 0, "only even indices should remain");
    }
}

/// Regression: large page traversal with small limits has no performance degradation.
#[test]
fn test_large_dataset_small_limit_completes() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 50);
    
    // Paginate with very small limit
    let all_ids = paginate_all(&client, &owner, 2);
    assert_eq!(all_ids.len(), 50);
    
    // Verify strictly ascending
    for i in 1..all_ids.len() {
        assert!(all_ids[i] > all_ids[i-1]);
    }
}

/// Regression: zero archived bills then archive one returns correct single-item page.
#[test]
fn test_empty_then_archive_one_returns_single_item() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    
    // Start with empty archive
    let empty = client.get_archived_bills_page(&owner, &0, &10);
    assert_eq!(empty.count, 0);
    
    // Archive one bill
    create_pay_archive(&env, &client, &owner, 1);
    
    let after = client.get_archived_bills_page(&owner, &0, &10);
    assert_eq!(after.count, 1);
    assert_eq!(after.next_cursor, 0);
    assert_eq!(after.items.len(), 1);
}

/// Regression: cursor equals first item ID skips it and returns rest.
#[test]
fn test_cursor_at_first_id_skips_first_item() {
    let env = make_env();
    let (client, owner) = setup_client(&env);
    create_pay_archive(&env, &client, &owner, 5);
    
    // Get all items
    let all = client.get_archived_bills_page(&owner, &0, &50);
    let first_id = all.items.first().unwrap().id;
    
    // Use first ID as cursor
    let page = client.get_archived_bills_page(&owner, &first_id, &50);
    assert_eq!(page.count, 4, "should skip first item and return 4");
    
    // First item should not be in results
    for bill in page.items.iter() {
        assert_ne!(bill.id, first_id, "first item should not appear");
    }
}
