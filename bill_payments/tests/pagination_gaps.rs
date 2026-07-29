//! Pagination stability tests for bill_payments under sparse IDs and archive gaps.
//!
//! Issue #516: SC-063 Bill Payments: Add tests for pagination stability under
//! sparse IDs and archived gaps.
//!
//! Coverage:
//!   - No duplicates or skips when IDs are sparse due to archiving
//!   - Cursors remain stable across multiple page steps
//!   - Archived bills are excluded from unpaid pages
//!   - Restored bills re-appear in unpaid pages at the correct cursor position
//!   - Multi-page traversal collects exactly the expected set of bills
//!   - Boundary: at-the-limit, one-before, one-after pagination with gaps (#1135)

use bill_payments::{BillPayments, BillPaymentsClient};
use proptest::prelude::*;
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String};

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

fn setup(env: &Env) -> (BillPaymentsClient<'_>, Address) {
    let id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(env, &id);
    let owner = Address::generate(env);
    (client, owner)
}

fn create_bill(env: &Env, client: &BillPaymentsClient, owner: &Address) -> u32 {
    client.create_bill(
        owner,
        &String::from_str(env, "Bill"),
        &100i128,
        &2_000_000_000u64,
        &false,
        &0u32,
        &None,
        &String::from_str(env, "XLM"),
        &None,
    )
}

/// Collect all unpaid bill IDs via full cursor traversal.
fn collect_all_ids(client: &BillPaymentsClient, owner: &Address) -> std::vec::Vec<u32> {
    let mut ids = std::vec::Vec::new();
    let mut cursor = 0u32;
    loop {
        let page = client.get_unpaid_bills(owner, &cursor, &50u32);
        for bill in page.items.iter() {
            ids.push(bill.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }
    ids
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Archiving a subset of bills creates ID gaps; pagination must not duplicate
/// or skip the remaining unpaid bills.
#[test]
fn test_no_duplicates_or_skips_after_archive_gaps() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Create 10 bills (IDs 1..=10)
    for _ in 0..10 {
        create_bill(&env, &client, &owner);
    }

    // Pay bills 2, 4, 6, 8 so they can be archived
    for id in [2u32, 4, 6, 8] {
        client.pay_bill(&owner, &id);
    }

    // Archive all paid bills — creates gaps at IDs 2, 4, 6, 8
    client.archive_paid_bills(&owner, &2_000_000_001u64);

    // Remaining unpaid: 1, 3, 5, 7, 9, 10
    let ids = collect_all_ids(&client, &owner);
    assert_eq!(ids.len(), 6, "expected 6 unpaid bills after archiving 4");

    // Verify no duplicates
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            assert_ne!(ids[i], ids[j], "duplicate bill ID in pagination");
        }
    }

    // Verify exact set
    assert_eq!(ids, vec![1u32, 3, 5, 7, 9, 10]);
}

/// Cursor is stable: resuming from a saved cursor after archiving more bills
/// must not re-deliver already-seen bills.
#[test]
fn test_cursor_stable_across_archive_operations() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Create 12 bills (IDs 1..=12)
    for _ in 0..12 {
        create_bill(&env, &client, &owner);
    }

    // Fetch first page of 5
    let page1 = client.get_unpaid_bills(&owner, &0u32, &5u32);
    assert_eq!(page1.count, 5);
    let saved_cursor = page1.next_cursor;
    assert!(saved_cursor > 0, "expected a next cursor after first page");

    // Collect IDs seen on page 1
    let seen_ids: std::vec::Vec<u32> = page1.items.iter().map(|b| b.id).collect();

    // Pay and archive some bills that are BEFORE the saved cursor
    client.pay_bill(&owner, &2u32);
    client.pay_bill(&owner, &4u32);
    client.archive_paid_bills(&owner, &2_000_000_001u64);

    // Resume from saved cursor — must not re-deliver IDs already seen
    let page2 = client.get_unpaid_bills(&owner, &saved_cursor, &50u32);
    for bill in page2.items.iter() {
        assert!(
            !seen_ids.contains(&bill.id),
            "bill ID {} was delivered twice",
            bill.id
        );
    }
}

/// Archived bills must not appear in unpaid bill pages.
#[test]
fn test_archived_bills_excluded_from_unpaid_pages() {
    let env = make_env();
    let (client, owner) = setup(&env);

    for _ in 0..6 {
        create_bill(&env, &client, &owner);
    }

    // Pay and archive bills 1, 3, 5
    for id in [1u32, 3, 5] {
        client.pay_bill(&owner, &id);
    }
    client.archive_paid_bills(&owner, &2_000_000_001u64);

    let ids = collect_all_ids(&client, &owner);
    // Only 2, 4, 6 should remain
    assert_eq!(ids.len(), 3);
    for &bill_id in &ids {
        assert!(
            [2u32, 4, 6].contains(&bill_id),
            "unexpected bill ID {} in unpaid pages",
            bill_id
        );
    }
}

/// Restored bills must re-appear in active bill pages at the correct cursor position
/// while remaining excluded from unpaid pages because restore preserves `paid = true`.
#[test]
fn test_restored_bill_reappears_in_correct_cursor_position() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Bills 1..=5
    for _ in 0..5 {
        create_bill(&env, &client, &owner);
    }

    // Pay and archive bill 3
    client.pay_bill(&owner, &3u32);
    client.archive_paid_bills(&owner, &2_000_000_001u64);

    // Restore bill 3 — it goes back into BILLS map but remains paid.
    client.restore_bill(&owner, &3u32);

    let unpaid_ids = collect_all_ids(&client, &owner);
    assert_eq!(
        unpaid_ids.len(),
        4,
        "restored paid bill should remain excluded from unpaid pages"
    );
    assert!(
        !unpaid_ids.contains(&3u32),
        "restored paid bill must not appear in unpaid pages"
    );

    let mut ids = std::vec::Vec::new();
    let mut cursor = 0u32;
    loop {
        let page = client.get_all_bills_for_owner(&owner, &cursor, &50u32);
        for bill in page.items.iter() {
            ids.push(bill.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(
        ids.len(),
        5,
        "restored bill should reappear in active bill pages"
    );
    assert!(
        ids.contains(&3u32),
        "restored bill ID 3 missing from active pages"
    );

    // IDs must be in ascending order (no cursor ordering violation)
    for i in 1..ids.len() {
        assert!(
            ids[i] > ids[i - 1],
            "pagination order violated at position {}",
            i
        );
    }
}

/// Multi-page traversal over a sparse ID space collects exactly the right bills
/// with no duplicates across page boundaries.
#[test]
fn test_multi_page_traversal_sparse_ids_no_duplicates() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Create 20 bills
    for _ in 0..20 {
        create_bill(&env, &client, &owner);
    }

    // Pay every other bill (even IDs) and archive them
    for id in (2u32..=20).step_by(2) {
        client.pay_bill(&owner, &id);
    }
    client.archive_paid_bills(&owner, &2_000_000_001u64);

    // 10 unpaid bills remain (odd IDs 1,3,5,...,19); traverse with page size 3
    let mut all_ids: std::vec::Vec<u32> = std::vec::Vec::new();
    let mut cursor = 0u32;
    let mut page_count = 0u32;
    loop {
        let page = client.get_unpaid_bills(&owner, &cursor, &3u32);
        assert!(page.count <= 3, "page count exceeded limit");
        for bill in page.items.iter() {
            all_ids.push(bill.id);
        }
        page_count += 1;
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(all_ids.len(), 10, "expected exactly 10 unpaid bills");
    assert_eq!(page_count, 4, "10 items / 3 per page = 4 pages");

    // No duplicates
    for i in 0..all_ids.len() {
        for j in (i + 1)..all_ids.len() {
            assert_ne!(
                all_ids[i], all_ids[j],
                "duplicate ID in multi-page traversal"
            );
        }
    }

    // All returned IDs must be odd (unpaid)
    for &id in &all_ids {
        assert_eq!(
            id % 2,
            1,
            "even (archived) ID {} appeared in unpaid pages",
            id
        );
    }
}

/// Paginating over archived bills after mixed archive/restore operations
/// must not include restored (active) bills.
#[test]
fn test_archived_page_excludes_restored_bills() {
    let env = make_env();
    let (client, owner) = setup(&env);

    for _ in 0..6 {
        create_bill(&env, &client, &owner);
    }

    // Pay and archive all 6
    for id in 1u32..=6 {
        client.pay_bill(&owner, &id);
    }
    client.archive_paid_bills(&owner, &2_000_000_001u64);

    // Restore bills 2 and 4 back to active
    client.restore_bill(&owner, &2u32);
    client.restore_bill(&owner, &4u32);

    // Archived page should only contain 1, 3, 5, 6
    let arch_page = client.get_archived_bills(&owner, &0u32, &50u32);
    assert_eq!(arch_page.count, 4, "expected 4 archived bills");
    for bill in arch_page.items.iter() {
        assert!(
            ![2u32, 4].contains(&bill.id),
            "restored bill ID {} still in archived page",
            bill.id
        );
    }
}

/// Empty result when cursor is past the last bill ID.
#[test]
fn test_empty_page_when_cursor_past_max_id() {
    let env = make_env();
    let (client, owner) = setup(&env);

    for _ in 0..3 {
        create_bill(&env, &client, &owner);
    }

    // Cursor beyond any existing ID
    let page = client.get_unpaid_bills(&owner, &9999u32, &10u32);
    assert_eq!(page.count, 0);
    assert_eq!(page.next_cursor, 0);
}

/// Bills belonging to a different owner must not appear in another owner's pages.
#[test]
fn test_owner_isolation_across_sparse_ids() {
    let env = make_env();
    let (client, owner_a) = setup(&env);
    let owner_b = Address::generate(&env);

    // Interleave bills for two owners
    create_bill(&env, &client, &owner_a); // ID 1
    create_bill(&env, &client, &owner_b); // ID 2
    create_bill(&env, &client, &owner_a); // ID 3
    create_bill(&env, &client, &owner_b); // ID 4
    create_bill(&env, &client, &owner_a); // ID 5

    let ids_a = collect_all_ids(&client, &owner_a);
    let ids_b = collect_all_ids(&client, &owner_b);

    assert_eq!(ids_a.len(), 3);
    assert_eq!(ids_b.len(), 2);

    // No overlap
    for &id in &ids_a {
        assert!(
            !ids_b.contains(&id),
            "owner isolation violated for ID {}",
            id
        );
    }
}

// ---------------------------------------------------------------------------
// Boundary tests: at-the-limit, one-before, one-after with gaps (#1135)
// ---------------------------------------------------------------------------

/// Helper: create `n` bills for `owner`, then cancel specific `cancel_ids`
/// to create gaps. Returns the IDs of remaining unpaid bills.
fn create_with_gaps(
    env: &Env,
    client: &BillPaymentsClient,
    owner: &Address,
    n: u32,
    cancel_ids: &[u32],
) -> std::vec::Vec<u32> {
    let mut all_ids = std::vec::Vec::new();
    for _ in 0..n {
        let id = create_bill(env, client, owner);
        all_ids.push(id);
    }
    for &cid in cancel_ids {
        if cid <= all_ids.len() as u32 && all_ids.contains(&cid) {
            client.cancel_bill(owner, &cid);
        }
    }
    all_ids
        .into_iter()
        .filter(|id| !cancel_ids.contains(id))
        .collect()
}

/// Collect all unpaid bill IDs via full cursor traversal with a given page size.
fn collect_all_with_limit(
    client: &BillPaymentsClient,
    owner: &Address,
    limit: u32,
) -> std::vec::Vec<u32> {
    let mut ids = std::vec::Vec::new();
    let mut cursor = 0u32;
    loop {
        let page = client.get_unpaid_bills(owner, &cursor, &limit);
        for bill in page.items.iter() {
            ids.push(bill.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }
    ids
}

/// After cancellation gaps, exactly `limit` eligible bills remain.
/// Pagination must return all of them in a single page with next_cursor == 0.
#[test]
fn test_gap_boundary_exact_limit_after_gaps() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Create 6 bills, cancel 1 to leave exactly 5 (limit=5).
    // Cancelling bill 2 creates a gap.
    let remaining = create_with_gaps(&env, &client, &owner, 6, &[2u32]);
    assert_eq!(remaining.len(), 5, "precondition: 5 bills must remain");

    // Request with limit = 5 (exact match = at the limit)
    let page = client.get_unpaid_bills(&owner, &0, &5);
    assert_eq!(
        page.count, 5,
        "all 5 remaining bills must be returned in one page"
    );
    assert_eq!(
        page.next_cursor, 0,
        "next_cursor must be 0 when all items fit in one page"
    );
    let ids: std::vec::Vec<u32> = page.items.iter().map(|b| b.id).collect();
    assert_eq!(ids, remaining, "returned IDs must match the remaining set");
}

/// After cancellation gaps, exactly `limit - 1` eligible bills remain
/// (one before the limit). Pagination must return all in a single page.
#[test]
fn test_gap_boundary_limit_minus_one_after_gaps() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Create 5 bills, cancel 1 to leave 4 (limit-1 when limit=5).
    let remaining = create_with_gaps(&env, &client, &owner, 5, &[3u32]);
    assert_eq!(remaining.len(), 4, "precondition: 4 bills must remain");

    // Request with limit = 5 (one before the limit)
    let page = client.get_unpaid_bills(&owner, &0, &5);
    assert_eq!(
        page.count, 4,
        "all 4 remaining bills must be returned in one page"
    );
    assert_eq!(
        page.next_cursor, 0,
        "next_cursor must be 0 when fewer items than limit remain"
    );
}

/// After cancellation gaps, exactly `limit + 1` eligible bills remain
/// (one after the limit). First page returns `limit` items with a valid
/// next_cursor; second page returns the last item.
#[test]
fn test_gap_boundary_limit_plus_one_after_gaps() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Create 7 bills, cancel 1 to leave 6 (limit+1 when limit=5).
    let remaining = create_with_gaps(&env, &client, &owner, 7, &[4u32]);
    assert_eq!(remaining.len(), 6, "precondition: 6 bills must remain");

    // Page 1: should return exactly 5 items with a valid next_cursor
    let page1 = client.get_unpaid_bills(&owner, &0, &5);
    assert_eq!(page1.count, 5, "first page must return exactly 5 items");
    assert!(
        page1.next_cursor > 0,
        "must have a non-zero next_cursor when more items exist"
    );
    let page1_ids: std::vec::Vec<u32> = page1.items.iter().map(|b| b.id).collect();

    // Page 2: should return the remaining 1 item
    let page2 = client.get_unpaid_bills(&owner, &page1.next_cursor, &5);
    assert_eq!(page2.count, 1, "second page must return exactly 1 item");
    assert_eq!(page2.next_cursor, 0, "next_cursor must be 0 on last page");

    // Combined result must equal the full remaining set
    let mut combined = page1_ids;
    for bill in page2.items.iter() {
        combined.push(bill.id);
    }
    assert_eq!(
        combined, remaining,
        "combined pages must cover all remaining bills in order"
    );
}

/// Cursor positioned at a gap ID (cancelled bill) must skip to the next
/// valid item and not re-deliver the gap itself.
#[test]
fn test_gap_boundary_cursor_at_gap_id_skips_to_next_valid() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Create 5 bills, cancel bill 3.
    create_with_gaps(&env, &client, &owner, 5, &[3u32]);

    // Cursor at the gap ID 3 — must skip it and return IDs > 3
    let page = client.get_unpaid_bills(&owner, &3, &10);
    assert_eq!(page.count, 2, "must return items 4 and 5 after gap");
    let ids: std::vec::Vec<u32> = page.items.iter().map(|b| b.id).collect();
    assert_eq!(
        ids,
        vec![4u32, 5],
        "must skip the gap and return following items"
    );
    assert_eq!(page.next_cursor, 0);
}

/// Cursor at a gap ID where no further items exist returns empty page.
#[test]
fn test_gap_boundary_cursor_at_gap_id_no_remaining_returns_empty() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Create 3 bills, cancel bills 2 and 3.
    create_with_gaps(&env, &client, &owner, 3, &[2u32, 3u32]);

    // Only bill 1 remains. Cursor at 2 (gap) — no items after.
    let page = client.get_unpaid_bills(&owner, &2, &10);
    assert_eq!(page.count, 0, "no items remain after gap at cursor=2");
    assert_eq!(page.next_cursor, 0);
}

/// Multi-gap scenario with boundary pagination: create N items, cancel every
/// other to create sparse IDs, then traverse with various page sizes.
/// Covers the "at the limit, one before, one after" invariant for all page
/// sizes in the pagination loop.
#[test]
fn test_gap_boundary_multi_page_traversal_various_limits() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Create 10 bills, cancel 3 of them (IDs 2, 5, 8).
    let remaining = create_with_gaps(&env, &client, &owner, 10, &[2u32, 5u32, 8u32]);
    assert_eq!(remaining.len(), 7, "precondition: 7 bills must remain");

    for &page_limit in &[1u32, 2, 3, 4, 5, 6, 7, 8, 9, 10, 50] {
        let collected = collect_all_with_limit(&client, &owner, page_limit);
        assert_eq!(
            collected, remaining,
            "collected set must match remaining set at limit={}",
            page_limit
        );

        // Strictly ascending — no duplicates
        for i in 1..collected.len() {
            assert!(
                collected[i - 1] < collected[i],
                "items must be strictly ascending at limit={}; {} >= {}",
                page_limit,
                collected[i - 1],
                collected[i]
            );
        }

        // Verify no item outside the remaining set was returned
        for &id in &collected {
            assert!(
                remaining.contains(&id),
                "unexpected ID {} returned at limit={}",
                id,
                page_limit
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Property test: gap pagination invariants (#1135)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// For any combination of bills and cancelled bills (creating ID gaps),
    /// pagination with any page size must:
    ///   - Collect exactly the set of remaining unpaid bills
    ///   - Never return duplicates
    ///   - Always return items in strictly ascending ID order
    ///   - Correctly terminate with next_cursor == 0
    #[test]
    fn proptest_gap_pagination_invariants(
        n_total in 1u32..=30u32,
        n_cancel in 0u32..=10u32,
        page_size in 1u32..=15u32,
    ) {
        let env = make_env();
        let (client, owner) = setup(&env);

        // Create n_total bills
        let mut all_ids: std::vec::Vec<u32> = std::vec::Vec::new();
        for _ in 0..n_total {
            let id = create_bill(&env, &client, &owner);
            all_ids.push(id);
        }

        // Cancel up to n_cancel distinct bills (non-deterministic which ones)
        let cancel_count = n_cancel.min(n_total.saturating_sub(1));
        let mut cancel_ids: std::vec::Vec<u32> = std::vec::Vec::new();
        for i in 0..cancel_count {
            let idx = (i as usize) * 2 + 1; // cancel every other to create gaps
            if idx < all_ids.len() {
                let cid = all_ids[idx];
                client.cancel_bill(&owner, &cid);
                cancel_ids.push(cid);
            }
        }

        let expected_ids: std::vec::Vec<u32> = all_ids
            .iter()
            .copied()
            .filter(|id| !cancel_ids.contains(id))
            .collect();

        // Full pagination traversal
        let collected = collect_all_with_limit(&client, &owner, page_size);

        // Invariant 1: correct count
        prop_assert_eq!(
            collected.len(),
            expected_ids.len(),
            "collected {} items, expected {}",
            collected.len(),
            expected_ids.len()
        );

        // Invariant 2: no duplicates (strictly ascending implies no duplicates)
        for i in 1..collected.len() {
            prop_assert!(
                collected[i - 1] < collected[i],
                "non-ascending order at position {}: {} >= {}",
                i,
                collected[i - 1],
                collected[i]
            );
        }

        // Invariant 3: every collected ID is in the expected set
        for id in &collected {
            prop_assert!(
                expected_ids.contains(id),
                "collected ID {} not in expected set",
                id
            );
        }

        // Invariant 4: every expected ID was collected
        for id in &expected_ids {
            prop_assert!(
                collected.contains(id),
                "expected ID {} was never collected",
                id
            );
        }
    }
}
