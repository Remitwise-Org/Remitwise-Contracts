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

use bill_payments::{BillPayments, BillPaymentsClient};
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

fn setup(env: &Env) -> (BillPaymentsClient, Address) {
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
        client.pay_bill(&owner, &id).unwrap();
    }

    // Archive all paid bills — creates gaps at IDs 2, 4, 6, 8
    client
        .archive_paid_bills(&owner, &2_000_000_001u64)
        .unwrap();

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
    client.pay_bill(&owner, &2u32).unwrap();
    client.pay_bill(&owner, &4u32).unwrap();
    client
        .archive_paid_bills(&owner, &2_000_000_001u64)
        .unwrap();

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
        client.pay_bill(&owner, &id).unwrap();
    }
    client
        .archive_paid_bills(&owner, &2_000_000_001u64)
        .unwrap();

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

/// Restored bills must re-appear in unpaid pages at the correct cursor position.
#[test]
fn test_restored_bill_reappears_in_correct_cursor_position() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Bills 1..=5
    for _ in 0..5 {
        create_bill(&env, &client, &owner);
    }

    // Pay and archive bill 3
    client.pay_bill(&owner, &3u32).unwrap();
    client
        .archive_paid_bills(&owner, &2_000_000_001u64)
        .unwrap();

    // Restore bill 3 — it goes back into BILLS map
    client.restore_bill(&owner, &3u32).unwrap();

    let ids = collect_all_ids(&client, &owner);
    // All 5 bills should be present (bill 3 is restored but marked unpaid)
    assert_eq!(ids.len(), 5, "restored bill should reappear in unpaid pages");
    assert!(ids.contains(&3u32), "restored bill ID 3 missing from pages");

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
        client.pay_bill(&owner, &id).unwrap();
    }
    client
        .archive_paid_bills(&owner, &2_000_000_001u64)
        .unwrap();

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
            assert_ne!(all_ids[i], all_ids[j], "duplicate ID in multi-page traversal");
        }
    }

    // All returned IDs must be odd (unpaid)
    for &id in &all_ids {
        assert_eq!(id % 2, 1, "even (archived) ID {} appeared in unpaid pages", id);
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
        client.pay_bill(&owner, &id).unwrap();
    }
    client
        .archive_paid_bills(&owner, &2_000_000_001u64)
        .unwrap();

    // Restore bills 2 and 4 back to active
    client.restore_bill(&owner, &2u32).unwrap();
    client.restore_bill(&owner, &4u32).unwrap();

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
        assert!(!ids_b.contains(&id), "owner isolation violated for ID {}", id);
    }
}
