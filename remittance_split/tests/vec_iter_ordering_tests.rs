#![cfg(test)]

//! Regression tests that pin Soroban `Vec` iteration ordering for the
//! per-owner schedule index in `remittance_split`.
//!
//! The contract relies on `Vec::iter()` yielding the owner's schedule IDs in
//! **insertion order**: `create_remittance_schedule` appends each new ID with
//! `push_back` (see the invariant comment in `lib.rs`), and the read paths
//! (`get_remittance_schedules` / `get_schedules_paginated`) walk that `Vec`
//! with `iter()` and return the schedules in the order they are visited.
//!
//! Soroban's `Vec` preserves insertion order today, but nothing in the type
//! signature guarantees it, so these tests lock the behaviour in place:
//!
//! - happy path: schedules come back in the order they were created;
//! - boundary / sad path: iteration follows *insertion* order, not a value
//!   sort — schedules created with descending amounts are returned with
//!   amounts still descending, proving the `Vec` is never silently sorted;
//! - pagination: page-by-page traversal preserves the same ascending-ID order.

use remittance_split::{RemittanceSplit, RemittanceSplitClient};
use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env};

/// Register the contract and initialise a split for a freshly generated owner.
///
/// Returns the client and the owner address. A dummy token address is fine
/// here: these tests only exercise the schedule index, never a distribution.
fn setup(env: &Env) -> (RemittanceSplitClient<'_>, Address) {
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(env, &contract_id);

    let owner = Address::generate(env);
    let token = Address::generate(env);
    // percentages in basis points: 50% / 30% / 15% / 5%
    client.initialize_split(&owner, &0, &token, &5000, &3000, &1500, &500);

    (client, owner)
}

#[test]
fn get_remittance_schedules_returns_ids_in_insertion_order() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    // Create five schedules. IDs are allocated from a monotonic counter, so
    // insertion order is 1, 2, 3, 4, 5.
    let mut created_ids = std::vec::Vec::new();
    for i in 1..=5u64 {
        let id = client.create_remittance_schedule(
            &owner,
            &(100 * i as i128),
            &(2_000 + i * 1_000), // strictly in the future, well under the lead-time cap
            &0,                   // one-off schedule
        );
        created_ids.push(id);
    }
    assert_eq!(created_ids, std::vec![1, 2, 3, 4, 5]);

    let schedules = client.get_remittance_schedules(&owner);
    assert_eq!(schedules.len(), 5, "all five schedules should be returned");

    // The Vec must iterate in insertion order: ids 1..=5 and the matching
    // amounts 100, 200, 300, 400, 500.
    for (idx, schedule) in schedules.iter().enumerate() {
        let expected_id = idx as u32 + 1;
        assert_eq!(
            schedule.id, expected_id,
            "schedule at position {idx} should have id {expected_id}"
        );
        assert_eq!(
            schedule.amount,
            100 * (idx as i128 + 1),
            "amount at position {idx} should follow insertion order"
        );
    }
}

#[test]
fn get_remittance_schedules_preserves_insertion_order_not_value_sort() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    // Insert schedules with strictly *descending* amounts. If iteration ever
    // returned a value-sorted view instead of insertion order, the amounts
    // below would come back ascending and this test would fail.
    let descending_amounts = [500i128, 400, 300, 200, 100];
    for (i, amount) in descending_amounts.iter().enumerate() {
        let id = client.create_remittance_schedule(
            &owner,
            amount,
            &(2_000 + (i as u64 + 1) * 1_000),
            &0,
        );
        // IDs are still allocated ascending regardless of amount.
        assert_eq!(id, i as u32 + 1);
    }

    let schedules = client.get_remittance_schedules(&owner);
    assert_eq!(schedules.len(), 5);

    let returned_amounts: std::vec::Vec<i128> = schedules.iter().map(|s| s.amount).collect();
    assert_eq!(
        returned_amounts,
        std::vec![500, 400, 300, 200, 100],
        "iteration must follow insertion order, never a value sort"
    );

    // IDs remain ascending (insertion order) even though amounts descend.
    let returned_ids: std::vec::Vec<u32> = schedules.iter().map(|s| s.id).collect();
    assert_eq!(returned_ids, std::vec![1, 2, 3, 4, 5]);
}

#[test]
fn paginated_schedules_preserve_ascending_id_order_across_pages() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    // Create six schedules so we get multiple non-trivial pages of two.
    for i in 1..=6u64 {
        client.create_remittance_schedule(
            &owner,
            &(100 * i as i128),
            &(2_000 + i * 1_000),
            &0,
        );
    }

    // Walk every page using the returned cursor and flatten the IDs. The
    // concatenated order must match the insertion order exactly.
    let page_size = 2u32;
    let mut collected_ids = std::vec::Vec::new();
    let mut cursor = 0u32;
    loop {
        let page = client.get_schedules_paginated(&owner, &cursor, &page_size);
        assert!(
            page.items.len() <= page_size,
            "page must not exceed the requested limit"
        );
        for schedule in page.items.iter() {
            collected_ids.push(schedule.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(
        collected_ids,
        std::vec![1, 2, 3, 4, 5, 6],
        "paginated traversal must preserve insertion (ascending-id) order"
    );
}

#[test]
fn get_remittance_schedules_returns_empty_for_owner_without_schedules() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    // No schedules created: the iteration source is empty and must stay empty.
    let schedules = client.get_remittance_schedules(&owner);
    assert_eq!(schedules.len(), 0);
}
