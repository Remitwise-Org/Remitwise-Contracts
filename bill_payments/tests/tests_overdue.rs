//! Correctness tests for `get_overdue_bills`.
//!
//! Documented boundary: a bill is overdue when `due_date < current_ledger_time`.
//! Equality (`due_date == now`) is **not** overdue — the filter is strict less-than.
//!
//! Coverage (issue #740):
//!   - Boundary inclusivity: due_date == now (not overdue), due_date == now-1 (overdue),
//!     due_date == now+1 (not overdue); all three present simultaneously.
//!   - Paid bills excluded from overdue list.
//!   - Cancelled bills excluded from overdue list.
//!   - Archived bills excluded from overdue list.
//!   - `BillPage` cursor, count, and ID ordering are stable across pages with sparse IDs.
//!   - Owner isolation: bills carry the correct owner; no cross-contamination.

use bill_payments::{BillPayments, BillPaymentsClient};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String};

// ─────────────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────────────

const BASE_TIME: u64 = 2_000_000;

fn make_env(timestamp: u64) -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    set_time(&env, timestamp);
    env.budget().reset_unlimited();
    env
}

fn set_time(env: &Env, timestamp: u64) {
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 1,
        timestamp,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 3_000_000,
    });
}

fn setup_contract(env: &Env) -> BillPaymentsClient<'_> {
    let id = env.register_contract(None, BillPayments);
    BillPaymentsClient::new(env, &id)
}

fn create_bill(env: &Env, client: &BillPaymentsClient, owner: &Address, due_date: u64) -> u32 {
    client.create_bill(
        owner,
        &String::from_str(env, "Test Bill"),
        &100i128,
        &due_date,
        &false,
        &0u32,
        &None,
        &String::from_str(env, "XLM"),
        &None,
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Boundary: due_date == now, now-1, now+1
// ─────────────────────────────────────────────────────────────────────────────

/// A bill with `due_date == now` is NOT overdue.
/// The filter `due_date < current_time` is strict less-than, so equality is on-time.
#[test]
fn test_overdue_due_date_equals_now_not_overdue() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    create_bill(&env, &client, &owner, BASE_TIME);

    let page = client.get_overdue_bills(&0, &100);
    assert_eq!(
        page.count, 0,
        "due_date == now must NOT appear in overdue list"
    );
}

/// A bill with `due_date == now - 1` IS overdue.
#[test]
fn test_overdue_due_date_one_second_before_now_is_overdue() {
    let due_date = BASE_TIME - 1;

    // Create the bill while time is still at due_date (passes the >= check).
    let env = make_env(due_date);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);
    create_bill(&env, &client, &owner, due_date);

    // Advance one second: now due_date < current_time.
    set_time(&env, BASE_TIME);
    let page = client.get_overdue_bills(&0, &100);
    assert_eq!(
        page.count, 1,
        "due_date == now - 1 must appear in overdue list"
    );
    assert!(
        page.items.get(0).unwrap().due_date < BASE_TIME,
        "returned bill's due_date must be strictly less than current_time"
    );
}

/// A bill with `due_date == now + 1` is NOT overdue.
#[test]
fn test_overdue_due_date_one_second_after_now_not_overdue() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    create_bill(&env, &client, &owner, BASE_TIME + 1);

    let page = client.get_overdue_bills(&0, &100);
    assert_eq!(
        page.count, 0,
        "due_date == now + 1 must NOT appear in overdue list"
    );
}

/// Three-way boundary: now-1 (overdue), now (not overdue), now+1 (not overdue).
/// All three bills exist simultaneously; only the one behind the clock is overdue.
#[test]
fn test_overdue_three_way_boundary_now_minus_one_now_now_plus_one() {
    // Create the "past" bill one second behind.
    let env = make_env(BASE_TIME - 1);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    create_bill(&env, &client, &owner, BASE_TIME - 1);

    // Advance to BASE_TIME; create the boundary and future bills.
    set_time(&env, BASE_TIME);
    create_bill(&env, &client, &owner, BASE_TIME);
    create_bill(&env, &client, &owner, BASE_TIME + 1);

    let page = client.get_overdue_bills(&0, &100);
    assert_eq!(
        page.count, 1,
        "only the bill with due_date < now must appear overdue"
    );
    assert!(
        page.items.get(0).unwrap().due_date < BASE_TIME,
        "the overdue bill's due_date must be strictly before current_time"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Status exclusion: paid, cancelled, archived
// ─────────────────────────────────────────────────────────────────────────────

/// A paid bill is excluded from the overdue list even when `due_date < now`.
#[test]
fn test_overdue_excludes_paid_bills() {
    let due_date = BASE_TIME - 1;

    let env = make_env(due_date);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    let bill_id = create_bill(&env, &client, &owner, due_date);

    set_time(&env, BASE_TIME);
    client.pay_bill(&owner, &bill_id);

    let page = client.get_overdue_bills(&0, &100);
    assert_eq!(page.count, 0, "paid bill must not appear in overdue list");
}

/// A cancelled bill is excluded: `cancel_bill` removes the entry from storage entirely.
#[test]
fn test_overdue_excludes_cancelled_bills() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    // Create at exactly now (valid). Cancel it. Advance time so it would be overdue.
    let bill_id = create_bill(&env, &client, &owner, BASE_TIME);
    client.cancel_bill(&owner, &bill_id);

    set_time(&env, BASE_TIME + 1);
    let page = client.get_overdue_bills(&0, &100);
    assert_eq!(
        page.count, 0,
        "cancelled bill must not appear in overdue list"
    );
}

/// An archived bill is excluded: `archive_paid_bills` moves bills to `ARCH_BILL` storage
/// which `get_overdue_bills` never queries.
#[test]
fn test_overdue_excludes_archived_bills() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    let bill_id = create_bill(&env, &client, &owner, BASE_TIME);

    // Pay the bill so it qualifies for archival.
    client.pay_bill(&owner, &bill_id);

    // Archive all bills with paid_at < BASE_TIME + 10.
    set_time(&env, BASE_TIME + 10);
    client.archive_paid_bills(&owner, &(BASE_TIME + 10));

    let page = client.get_overdue_bills(&0, &100);
    assert_eq!(
        page.count, 0,
        "archived bill must not appear in overdue list"
    );
}

/// All bills paid: overdue list must be empty even when every bill has a past due_date.
#[test]
fn test_overdue_empty_when_all_bills_paid() {
    let due_date = BASE_TIME - 1;

    let env = make_env(due_date);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    let id1 = create_bill(&env, &client, &owner, due_date);
    let id2 = create_bill(&env, &client, &owner, due_date);

    set_time(&env, BASE_TIME);
    client.pay_bill(&owner, &id1);
    client.pay_bill(&owner, &id2);

    let page = client.get_overdue_bills(&0, &100);
    assert_eq!(page.count, 0, "all bills paid: overdue list must be empty");
}

// ─────────────────────────────────────────────────────────────────────────────
// Pagination: cursor, count, ordering
// ─────────────────────────────────────────────────────────────────────────────

/// Cursor-based pagination collects all overdue bills exactly once in ascending ID order.
///
/// Creates 5 overdue bills and traverses them with page size 2. Expects all 5
/// unique IDs to be returned in strictly ascending order without duplicates.
#[test]
fn test_overdue_pagination_stable_cursor_and_ordering() {
    let due_date = BASE_TIME - 1;

    let env = make_env(due_date);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    for _ in 0..5 {
        create_bill(&env, &client, &owner, due_date);
    }

    set_time(&env, BASE_TIME);

    let mut collected: std::vec::Vec<u32> = std::vec::Vec::new();
    let mut cursor = 0u32;
    loop {
        let page = client.get_overdue_bills(&cursor, &2);
        for bill in page.items.iter() {
            collected.push(bill.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(collected.len(), 5, "all 5 overdue bills must be collected");

    // IDs must be strictly ascending (no duplicates, stable ordering).
    for i in 1..collected.len() {
        assert!(
            collected[i - 1] < collected[i],
            "overdue bills must be returned in strictly ascending ID order"
        );
    }
}

/// Cancelled bills create ID gaps; pagination skips gaps without duplicating or missing bills.
#[test]
fn test_overdue_pagination_stable_across_sparse_ids() {
    let due_date = BASE_TIME - 1;

    let env = make_env(due_date);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    // Create 5 bills (IDs 1..=5), then cancel 2 and 4 to introduce gaps.
    for _ in 0..5 {
        create_bill(&env, &client, &owner, due_date);
    }
    client.cancel_bill(&owner, &2u32);
    client.cancel_bill(&owner, &4u32);

    set_time(&env, BASE_TIME);

    let page = client.get_overdue_bills(&0, &100);
    assert_eq!(
        page.count, 3,
        "3 non-cancelled overdue bills must be returned"
    );

    let mut ids: std::vec::Vec<u32> = std::vec::Vec::new();
    for bill in page.items.iter() {
        ids.push(bill.id);
    }
    assert_eq!(
        ids,
        std::vec![1u32, 3u32, 5u32],
        "only non-cancelled bill IDs must appear, in ascending order"
    );
}

/// `BillPage` fields are consistent: `count == items.len()`, `next_cursor == 0` on last page.
#[test]
fn test_overdue_page_fields_consistent() {
    let due_date = BASE_TIME - 1;

    let env = make_env(due_date);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    for _ in 0..3 {
        create_bill(&env, &client, &owner, due_date);
    }

    set_time(&env, BASE_TIME);

    let page = client.get_overdue_bills(&0, &100);
    assert_eq!(
        page.count,
        page.items.len(),
        "BillPage.count must equal items.len()"
    );
    assert_eq!(
        page.next_cursor, 0,
        "no further pages: next_cursor must be 0"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Owner isolation
// ─────────────────────────────────────────────────────────────────────────────

/// Owner isolation: two owners' overdue bills appear in the global list with
/// correct ownership — no bill carries the wrong owner address.
#[test]
fn test_overdue_owner_isolation_no_cross_contamination() {
    let due_date = BASE_TIME - 1;

    let env = make_env(due_date);
    let client = setup_contract(&env);
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);

    // Owner A: 2 overdue bills.
    create_bill(&env, &client, &owner_a, due_date);
    create_bill(&env, &client, &owner_a, due_date);

    // Owner B: 1 overdue bill.
    create_bill(&env, &client, &owner_b, due_date);

    set_time(&env, BASE_TIME);

    let page = client.get_overdue_bills(&0, &100);
    assert_eq!(
        page.count, 3,
        "all 3 overdue bills must appear in global list"
    );

    let mut a_count = 0u32;
    let mut b_count = 0u32;
    for bill in page.items.iter() {
        if bill.owner == owner_a {
            a_count += 1;
            assert_ne!(
                bill.owner, owner_b,
                "owner A's bill must not belong to owner B"
            );
        } else if bill.owner == owner_b {
            b_count += 1;
            assert_ne!(
                bill.owner, owner_a,
                "owner B's bill must not belong to owner A"
            );
        } else {
            panic!("unexpected owner in overdue list");
        }
    }
    assert_eq!(
        a_count, 2,
        "owner A must have 2 overdue bills in global list"
    );
    assert_eq!(
        b_count, 1,
        "owner B must have 1 overdue bill in global list"
    );
}

/// Paying one owner's overdue bill does not affect the other owner's overdue count.
#[test]
fn test_overdue_owner_isolation_payment_does_not_affect_other_owner() {
    let due_date = BASE_TIME - 1;

    let env = make_env(due_date);
    let client = setup_contract(&env);
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);

    let a_bill = create_bill(&env, &client, &owner_a, due_date);
    create_bill(&env, &client, &owner_b, due_date);

    set_time(&env, BASE_TIME);

    // Owner A pays their bill.
    client.pay_bill(&owner_a, &a_bill);

    let page = client.get_overdue_bills(&0, &100);
    assert_eq!(
        page.count, 1,
        "only owner B's bill must remain overdue after A pays"
    );
    assert_eq!(
        page.items.get(0).unwrap().owner,
        owner_b,
        "the remaining overdue bill must belong to owner B"
    );
}

/// Global overdue list merges multiple owners' bills into one strictly ascending,
/// duplicate-free page — verifying the owner-index merge preserves canonical ordering.
#[test]
fn test_overdue_global_merges_owners_in_ascending_order() {
    let due_date = BASE_TIME - 1;

    let env = make_env(due_date);
    let client = setup_contract(&env);
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);

    // Interleave creation so per-owner index lists do not line up with global order:
    // IDs 1,3,5 -> A ; IDs 2,4,6 -> B.
    create_bill(&env, &client, &owner_a, due_date); // 1
    create_bill(&env, &client, &owner_b, due_date); // 2
    create_bill(&env, &client, &owner_a, due_date); // 3
    create_bill(&env, &client, &owner_b, due_date); // 4
    create_bill(&env, &client, &owner_a, due_date); // 5
    create_bill(&env, &client, &owner_b, due_date); // 6

    set_time(&env, BASE_TIME);

    // Page through with a small limit to exercise the bounded merge + cursor.
    let mut collected: std::vec::Vec<u32> = std::vec::Vec::new();
    let mut cursor = 0u32;
    loop {
        let page = client.get_overdue_bills(&cursor, &2);
        for bill in page.items.iter() {
            collected.push(bill.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(
        collected,
        std::vec![1u32, 2, 3, 4, 5, 6],
        "global overdue page must merge both owners in strictly ascending ID order"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Owner-scoped variant: get_overdue_bills_for_owner
// ─────────────────────────────────────────────────────────────────────────────

/// Owner-scoped query returns only the requested owner's overdue bills.
#[test]
fn test_overdue_for_owner_scopes_to_single_owner() {
    let due_date = BASE_TIME - 1;

    let env = make_env(due_date);
    let client = setup_contract(&env);
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);

    create_bill(&env, &client, &owner_a, due_date); // 1 (A)
    create_bill(&env, &client, &owner_b, due_date); // 2 (B)
    create_bill(&env, &client, &owner_a, due_date); // 3 (A)

    set_time(&env, BASE_TIME);

    let page = client.get_overdue_bills_for_owner(&owner_a, &0, &100);
    assert_eq!(page.count, 2, "owner A has exactly 2 overdue bills");
    let mut ids: std::vec::Vec<u32> = std::vec::Vec::new();
    for bill in page.items.iter() {
        assert_eq!(bill.owner, owner_a, "only owner A's bills may appear");
        ids.push(bill.id);
    }
    assert_eq!(ids, std::vec![1u32, 3u32], "ascending owner-scoped IDs");
}

/// Owner-scoped boundary matches the global semantics: now-1 overdue; now / now+1 not.
#[test]
fn test_overdue_for_owner_boundary_strict_less_than() {
    let env = make_env(BASE_TIME - 1);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    create_bill(&env, &client, &owner, BASE_TIME - 1); // overdue once clock advances

    set_time(&env, BASE_TIME);
    create_bill(&env, &client, &owner, BASE_TIME); // due_date == now -> NOT overdue
    create_bill(&env, &client, &owner, BASE_TIME + 1); // future -> NOT overdue

    let page = client.get_overdue_bills_for_owner(&owner, &0, &100);
    assert_eq!(page.count, 1, "only the bill with due_date < now is overdue");
    assert!(page.items.get(0).unwrap().due_date < BASE_TIME);
}

/// Owner-scoped query excludes paid bills and survives sparse IDs from cancellation.
#[test]
fn test_overdue_for_owner_excludes_paid_and_handles_gaps() {
    let due_date = BASE_TIME - 1;

    let env = make_env(due_date);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    for _ in 0..5 {
        create_bill(&env, &client, &owner, due_date); // IDs 1..=5
    }
    // Cancel 2 and 4 (sparse IDs), pay 5 (excluded as paid).
    client.cancel_bill(&owner, &2u32);
    client.cancel_bill(&owner, &4u32);

    set_time(&env, BASE_TIME);
    client.pay_bill(&owner, &5u32);

    let page = client.get_overdue_bills_for_owner(&owner, &0, &100);
    let mut ids: std::vec::Vec<u32> = std::vec::Vec::new();
    for bill in page.items.iter() {
        ids.push(bill.id);
    }
    assert_eq!(
        ids,
        std::vec![1u32, 3u32],
        "cancelled (2,4) and paid (5) bills excluded; ascending order preserved"
    );
}

/// Owner-scoped pagination is cursor-stable across pages.
#[test]
fn test_overdue_for_owner_pagination_stable_cursor() {
    let due_date = BASE_TIME - 1;

    let env = make_env(due_date);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    for _ in 0..5 {
        create_bill(&env, &client, &owner, due_date);
    }

    set_time(&env, BASE_TIME);

    let mut collected: std::vec::Vec<u32> = std::vec::Vec::new();
    let mut cursor = 0u32;
    loop {
        let page = client.get_overdue_bills_for_owner(&owner, &cursor, &2);
        for bill in page.items.iter() {
            collected.push(bill.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(collected, std::vec![1u32, 2, 3, 4, 5]);
}

/// Owner-scoped query returns an empty page for an owner with no overdue bills.
#[test]
fn test_overdue_for_owner_empty_when_none_overdue() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    create_bill(&env, &client, &owner, BASE_TIME + 1_000); // future, never overdue here

    let page = client.get_overdue_bills_for_owner(&owner, &0, &100);
    assert_eq!(page.count, 0);
    assert_eq!(page.next_cursor, 0);
}

/// Bill due far in the past still appears overdue (no age limit on overdue).
#[test]
fn test_overdue_bill_due_far_in_past_is_overdue() {
    let old_due = 1u64; // epoch + 1 second

    let env = make_env(old_due);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    create_bill(&env, &client, &owner, old_due);

    // Advance time by a large amount.
    set_time(&env, BASE_TIME);

    let page = client.get_overdue_bills(&0, &100);
    assert_eq!(
        page.count, 1,
        "a bill due far in the past must still appear overdue"
    );
}
