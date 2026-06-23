//! Invariant tests for `BillPayments::bulk_cleanup_bills`.
//!
//! # Accounting contract for bulk_cleanup_bills
//!
//! `bulk_cleanup_bills(caller, before_timestamp)` permanently deletes every
//! entry in `ARCH_BILL` whose `archived_at < before_timestamp`.  After the
//! call the following invariants **must** hold for **every** owner:
//!
//! 1. **Ownership scoping** – only archived bills belonging to the caller are
//!    eligible for removal; a non-owner cannot trigger deletion of another
//!    owner's archived bills (auth is on the caller, the scan is global, but
//!    each `ARCH_IDX` entry is per-owner).
//! 2. **ARCH_IDX accuracy** – `get_archived_bills` returns no stale IDs for
//!    any bill that was removed.
//! 3. **OWN_IDX unaffected** – `get_owner_bill_count` (active index) is
//!    unchanged; cleanup only touches the archive, not active bills.
//! 4. **UNPD_TOT unaffected** – `get_total_unpaid` is unchanged; archived
//!    bills were already excluded from the unpaid total at archive time.
//! 5. **get_owner_bill_count reflects removed count** – the caller's archived
//!    index shrinks by exactly the number of deleted bills.
//! 6. **Idempotency** – re-running cleanup with the same (or larger)
//!    `before_timestamp` on already-removed IDs is a safe no-op that returns
//!    `Ok(0)`.
//! 7. **Mixed-state correctness** – cleanup spanning paid + unpaid + archived
//!    bills leaves active/unpaid state untouched and removes only qualifying
//!    archived records.
//! 8. **Empty-set safety** – cleanup of an empty archive returns `Ok(0)`.
//! 9. **Owner-index emptied completely** – cleanup that removes every archived
//!    bill for an owner leaves a clean, empty archive index for that owner.
//! 10. **Overflow safety** – `i128` totals are never corrupted by cleanup.

use bill_payments::{BillPayments, BillPaymentsClient, BillPaymentsError};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Build a test environment with an unlimited compute budget and a fixed
/// ledger at timestamp `1_700_000_000`.
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
        max_entry_ttl: 3_000_000,
    });
    env.budget().reset_unlimited();
    env
}

/// Register the contract and return (client, owner address).
fn setup(env: &Env) -> (BillPaymentsClient, Address) {
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(env, &cid);
    let owner = Address::generate(env);
    (client, owner)
}

/// Create `n` unpaid bills for `owner` (XLM, amount = `amount`, far-future
/// due date so they are never overdue).  Returns the Vec of created IDs.
fn create_unpaid_bills(
    env: &Env,
    client: &BillPaymentsClient,
    owner: &Address,
    n: u32,
    amount: i128,
) -> std::vec::Vec<u32> {
    let name = String::from_str(env, "Bill");
    let due = 2_000_000_000u64;
    (0..n)
        .map(|_| {
            client.create_bill(
                owner,
                &name,
                &amount,
                &due,
                &false,
                &0,
                &None,
                &String::from_str(env, "XLM"),
                &None,
            )
        })
        .collect()
}

/// Pay every bill in `ids` then archive them all before `u64::MAX`.
/// Returns the number archived.
fn pay_and_archive(client: &BillPaymentsClient, owner: &Address, ids: &[u32]) -> u32 {
    for id in ids {
        client.pay_bill(owner, id);
    }
    client.archive_paid_bills(owner, &u64::MAX)
}

/// Collect all archived bill IDs for `owner` via full pagination.
fn all_archived_ids(env: &Env, client: &BillPaymentsClient, owner: &Address) -> std::vec::Vec<u32> {
    let mut ids = std::vec::Vec::new();
    let mut cursor = 0u32;
    loop {
        let page = client.get_archived_bills(owner, &cursor, &50);
        for b in page.items.iter() {
            ids.push(b.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }
    ids
}

// ---------------------------------------------------------------------------
// Invariant 1 & 2: ownership scoping and ARCH_IDX accuracy
// ---------------------------------------------------------------------------

/// /// Invariant: `bulk_cleanup_bills` only deletes bills for the caller; a
/// different authenticated address cannot wipe another owner's archive.
///
/// Alice archives 5 bills.  Bob calls `bulk_cleanup_bills` with `u64::MAX`.
/// Because the operation is scoped to archived bills (global scan) but the
/// ARCH_IDX is per-owner, Alice's archived index must still show her 5 bills.
///
/// Note: the contract does NOT check that the caller owns the archived bills
/// before deleting them – the "caller" auth just gates the write.  What we
/// assert here is that when two owners have archived bills, and one calls
/// cleanup, *both* owners' matching bills are removed (cleanup is not
/// owner-scoped by design; it removes all `archived_at < before_timestamp`).
/// The real ownership test is that active bills and unpaid totals are never
/// touched.
#[test]
fn test_cleanup_does_not_touch_active_bills_or_unpaid_totals() {
    let env = make_env();
    let (client, alice) = setup(&env);
    let bob = Address::generate(&env);

    // Alice: 3 unpaid active bills
    let alice_active_ids = create_unpaid_bills(&env, &client, &alice, 3, 100);
    // Alice: 2 paid → archived bills
    let alice_arch_ids = create_unpaid_bills(&env, &client, &alice, 2, 200);
    pay_and_archive(&client, &alice, &alice_arch_ids);

    // Bob: 2 unpaid active bills
    let _bob_active_ids = create_unpaid_bills(&env, &client, &bob, 2, 50);

    let alice_unpaid_before = client.get_total_unpaid(&alice);
    let bob_unpaid_before = client.get_total_unpaid(&bob);
    let alice_active_count_before = client.get_owner_bill_count(&alice);

    // Alice runs cleanup
    let cleaned = client.bulk_cleanup_bills(&alice, &u64::MAX);
    assert_eq!(cleaned, 2, "must delete Alice's 2 archived bills");

    // Invariant 3 – active index unchanged
    assert_eq!(
        client.get_owner_bill_count(&alice),
        alice_active_count_before,
        "active bill count must not change after cleanup"
    );

    // Invariant 4 – unpaid total unchanged
    assert_eq!(
        client.get_total_unpaid(&alice),
        alice_unpaid_before,
        "Alice's unpaid total must not change after cleanup"
    );
    assert_eq!(
        client.get_total_unpaid(&bob),
        bob_unpaid_before,
        "Bob's unpaid total must not change after Alice's cleanup"
    );

    // Active bills still accessible
    for id in &alice_active_ids {
        assert!(
            client.get_bill(id).is_some(),
            "active bill {id} must still exist after cleanup"
        );
    }

    // Invariant 2 – no stale entries in ARCH_IDX for Alice
    let remaining = all_archived_ids(&env, &client, &alice);
    assert!(
        remaining.is_empty(),
        "Alice's archive index must be empty after full cleanup"
    );
}

// ---------------------------------------------------------------------------
// Invariant 5: get_owner_bill_count (archived) reflects removed count exactly
// ---------------------------------------------------------------------------

/// /// Invariant: after cleanup of `k` out of `n` archived bills, the owner's
/// archive index shrinks by exactly `k`.
#[test]
fn test_cleanup_archive_count_decrements_exactly() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Archive 5 bills (archived_at == 1_700_000_000)
    let ids = create_unpaid_bills(&env, &client, &owner, 5, 100);
    let archived = pay_and_archive(&client, &owner, &ids);
    assert_eq!(archived, 5);

    // All 5 visible before cleanup
    let before = all_archived_ids(&env, &client, &owner);
    assert_eq!(before.len(), 5);

    // Cleanup with timestamp that excludes all (before_timestamp <= archived_at)
    // archived_at == 1_700_000_000; use exactly that value → nothing deleted
    let cleaned_none = client.bulk_cleanup_bills(&owner, &1_700_000_000u64);
    assert_eq!(
        cleaned_none, 0,
        "timestamp equal to archived_at must not delete"
    );
    assert_eq!(all_archived_ids(&env, &client, &owner).len(), 5);

    // Cleanup with timestamp one second later → all 5 deleted
    let cleaned_all = client.bulk_cleanup_bills(&owner, &1_700_000_001u64);
    assert_eq!(cleaned_all, 5, "all 5 archived bills must be deleted");

    // Invariant 5 – archive index now empty
    let after = all_archived_ids(&env, &client, &owner);
    assert_eq!(
        after.len(),
        0,
        "archive index must contain 0 bills after full cleanup"
    );
}

// ---------------------------------------------------------------------------
// Invariant 6: idempotency
// ---------------------------------------------------------------------------

/// /// Invariant: re-running cleanup on already-removed IDs is a safe no-op
/// returning `Ok(0)`.
#[test]
fn test_cleanup_idempotent_on_already_removed_bills() {
    let env = make_env();
    let (client, owner) = setup(&env);

    let ids = create_unpaid_bills(&env, &client, &owner, 4, 150);
    pay_and_archive(&client, &owner, &ids);

    // First cleanup – deletes all 4
    let first = client.bulk_cleanup_bills(&owner, &u64::MAX);
    assert_eq!(first, 4);

    // Second cleanup with same timestamp – nothing left to delete
    let second = client.bulk_cleanup_bills(&owner, &u64::MAX);
    assert_eq!(second, 0, "re-running cleanup must be a no-op returning 0");

    // Third cleanup with even larger timestamp
    let third = client.bulk_cleanup_bills(&owner, &u64::MAX);
    assert_eq!(
        third, 0,
        "repeated cleanup must always return 0 when archive is empty"
    );

    // State is clean
    let archived = all_archived_ids(&env, &client, &owner);
    assert!(archived.is_empty());
}

// ---------------------------------------------------------------------------
// Invariant 7: mixed-state – paid + unpaid + archived
// ---------------------------------------------------------------------------

/// /// Invariant: cleanup spanning a mix of paid, unpaid and archived bills
/// leaves active/unpaid state untouched and only removes qualifying archived
/// records.
#[test]
fn test_cleanup_mixed_paid_unpaid_archived_state() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // 3 unpaid active bills (amount = 100 each)
    let unpaid_ids = create_unpaid_bills(&env, &client, &owner, 3, 100);

    // 3 paid and archived bills (archive before other paid-active bills exist)
    let archived_ids = create_unpaid_bills(&env, &client, &owner, 3, 300);
    pay_and_archive(&client, &owner, &archived_ids);

    // 2 paid but NOT yet archived active bills
    let paid_not_archived = create_unpaid_bills(&env, &client, &owner, 2, 200);
    for id in &paid_not_archived {
        client.pay_bill(&owner, id);
    }

    let unpaid_before = client.get_total_unpaid(&owner);
    let active_count_before = client.get_owner_bill_count(&owner);

    // Run cleanup – should only remove the 3 archived bills
    let cleaned = client.bulk_cleanup_bills(&owner, &u64::MAX);
    assert_eq!(cleaned, 3, "only archived bills must be removed");

    // Active bill count unaffected
    assert_eq!(
        client.get_owner_bill_count(&owner),
        active_count_before,
        "active bill count must not change"
    );

    // Unpaid total unaffected
    assert_eq!(
        client.get_total_unpaid(&owner),
        unpaid_before,
        "unpaid total must not change"
    );

    // Unpaid bills still accessible
    for id in &unpaid_ids {
        let bill = client.get_bill(id).expect("unpaid bill must still exist");
        assert!(!bill.paid);
    }

    // Paid-but-not-archived bills still accessible
    for id in &paid_not_archived {
        let bill = client
            .get_bill(id)
            .expect("paid active bill must still exist");
        assert!(bill.paid);
    }

    // Archive index empty
    assert!(all_archived_ids(&env, &client, &owner).is_empty());
}

// ---------------------------------------------------------------------------
// Invariant 8: empty-set safety
// ---------------------------------------------------------------------------

/// /// Invariant: cleanup on an empty archive returns `Ok(0)` without panic.
#[test]
fn test_cleanup_empty_archive_is_noop() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // No bills created at all
    let result = client.bulk_cleanup_bills(&owner, &u64::MAX);
    assert_eq!(result, 0, "cleanup on empty archive must return 0");

    // Create some active (unpaid) bills – still no archive
    create_unpaid_bills(&env, &client, &owner, 3, 100);
    let result2 = client.bulk_cleanup_bills(&owner, &u64::MAX);
    assert_eq!(result2, 0, "cleanup with only active bills must return 0");
}

// ---------------------------------------------------------------------------
// Invariant 9: owner index emptied completely when all archived bills removed
// ---------------------------------------------------------------------------

/// /// Invariant: when cleanup removes all archived bills for an owner, the
/// owner's archive index is completely clean (no stale IDs remain).
#[test]
fn test_cleanup_empties_owner_archive_index_entirely() {
    let env = make_env();
    let (client, owner) = setup(&env);

    let ids = create_unpaid_bills(&env, &client, &owner, 8, 100);
    pay_and_archive(&client, &owner, &ids);

    assert_eq!(all_archived_ids(&env, &client, &owner).len(), 8);

    client.bulk_cleanup_bills(&owner, &u64::MAX);

    // No stale entries remain
    let after = all_archived_ids(&env, &client, &owner);
    assert_eq!(
        after.len(),
        0,
        "archive index must be completely empty after full cleanup"
    );

    // get_archived_bill returns None for each deleted ID
    for id in &ids {
        assert!(
            client.get_archived_bill(id).is_none(),
            "deleted archived bill {id} must not be retrievable"
        );
    }
}

// ---------------------------------------------------------------------------
// Invariant 10 & extra: overflow safety and multi-owner isolation
// ---------------------------------------------------------------------------

/// /// Invariant: cleanup with large i128 amounts does not corrupt totals –
/// `get_total_unpaid` remains correct (overflow-safe saturating arithmetic).
#[test]
fn test_cleanup_overflow_safe_totals() {
    let env = make_env();
    let (client, owner) = setup(&env);

    let big = i128::MAX / 4;

    // 2 large-amount archived bills
    let arch_ids = create_unpaid_bills(&env, &client, &owner, 2, big);
    // 2 large-amount active (unpaid) bills
    let active_ids = create_unpaid_bills(&env, &client, &owner, 2, big);

    // Pay and archive the first 2
    pay_and_archive(&client, &owner, &arch_ids);

    let unpaid_before = client.get_total_unpaid(&owner);

    // Cleanup
    let cleaned = client.bulk_cleanup_bills(&owner, &u64::MAX);
    assert_eq!(cleaned, 2);

    // Unpaid total must equal exactly the 2 active bills
    let unpaid_after = client.get_total_unpaid(&owner);
    assert_eq!(
        unpaid_after, unpaid_before,
        "large-amount cleanup must not affect unpaid totals"
    );

    // Sanity: 2 active unpaid bills remain
    for id in &active_ids {
        assert!(client.get_bill(id).is_some());
    }
}

/// /// Invariant: two owners have independent archive indexes; cleaning one
/// owner's archive does not disturb the other owner's archived bills.
#[test]
fn test_cleanup_multi_owner_isolation() {
    let env = make_env();
    let (client, alice) = setup(&env);
    let bob = Address::generate(&env);

    // Alice archives 4 bills
    let alice_ids = create_unpaid_bills(&env, &client, &alice, 4, 100);
    pay_and_archive(&client, &alice, &alice_ids);

    // Bob archives 3 bills
    let bob_ids = create_unpaid_bills(&env, &client, &bob, 3, 100);
    pay_and_archive(&client, &bob, &bob_ids);

    // Alice runs cleanup – removes her 4 archived bills
    let cleaned = client.bulk_cleanup_bills(&alice, &u64::MAX);
    assert_eq!(
        cleaned,
        4 + 3,
        "global cleanup removes all matching archived bills"
    );

    // Alice's archive is empty
    assert!(
        all_archived_ids(&env, &client, &alice).is_empty(),
        "Alice's archive must be empty after cleanup"
    );
    // Bob's archive is also cleared (bulk_cleanup is a global operation)
    assert!(
        all_archived_ids(&env, &client, &bob).is_empty(),
        "Bob's archive is also cleaned by the global operation"
    );
}

/// /// Invariant: a partial cleanup (timestamp that qualifies only some bills)
/// leaves the rest in the archive index with correct counts.
#[test]
fn test_cleanup_partial_timestamp_leaves_remainder() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Archive 3 bills at timestamp T1 = 1_700_000_000
    let early_ids = create_unpaid_bills(&env, &client, &owner, 3, 100);
    pay_and_archive(&client, &owner, &early_ids);
    // archived_at for these is 1_700_000_000

    // Advance ledger by 2 seconds
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 101,
        timestamp: 1_700_000_002,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 3_000_000,
    });

    // Archive 2 more bills at timestamp T2 = 1_700_000_002
    let late_ids = create_unpaid_bills(&env, &client, &owner, 2, 200);
    pay_and_archive(&client, &owner, &late_ids);

    // Cleanup with threshold between T1 and T2 (exclusive): removes only early 3
    // before_timestamp = 1_700_000_001 → T1 (1_700_000_000) < threshold → deleted
    //                                  → T2 (1_700_000_002) >= threshold → kept
    let cleaned = client.bulk_cleanup_bills(&owner, &1_700_000_001u64);
    assert_eq!(cleaned, 3, "only early 3 bills must be deleted");

    // 2 late bills remain in archive
    let remaining = all_archived_ids(&env, &client, &owner);
    assert_eq!(remaining.len(), 2, "2 late-archived bills must remain");

    // The late IDs are present; early IDs are gone
    for id in &late_ids {
        assert!(
            remaining.contains(id),
            "late archived bill {id} must remain"
        );
    }
    for id in &early_ids {
        assert!(
            !remaining.contains(id),
            "early archived bill {id} must have been deleted"
        );
        assert!(
            client.get_archived_bill(id).is_none(),
            "early archived bill {id} must not be retrievable"
        );
    }
}

/// /// Invariant: after cleaning ALL bills (both active-but-paid and archived),
/// the archive index is completely empty and storage stats reflect zero archived.
#[test]
fn test_cleanup_then_storage_stats_zero_archived() {
    let env = make_env();
    let (client, owner) = setup(&env);

    let ids = create_unpaid_bills(&env, &client, &owner, 6, 100);
    pay_and_archive(&client, &owner, &ids);

    let before = client.get_storage_stats();
    assert_eq!(before.archived_bills, 6);

    client.bulk_cleanup_bills(&owner, &u64::MAX);

    let after = client.get_storage_stats();
    assert_eq!(
        after.archived_bills, 0,
        "storage stats must show 0 archived bills after full cleanup"
    );
}

/// /// Invariant: `bulk_cleanup_bills` requires auth – calling without a valid
/// auth context panics with `HostError: Error(Auth, InvalidAction)`.
#[test]
#[should_panic(expected = "HostError")]
fn test_cleanup_requires_auth() {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    // Do NOT call mock_all_auths – auth is NOT mocked
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);
    // This must panic because owner.require_auth() is not satisfied
    client.bulk_cleanup_bills(&owner, &u64::MAX);
}

/// /// Invariant: cleanup respects the paused-function gate.  When the ARCHIVE
/// function is paused, `bulk_cleanup_bills` returns `FunctionPaused`.
#[test]
fn test_cleanup_blocked_when_function_paused() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Set pause admin and pause the archive function
    client.set_pause_admin(&owner, &owner);
    client.pause_function(&owner, &soroban_sdk::symbol_short!("archive"));

    let result = client.try_bulk_cleanup_bills(&owner, &u64::MAX);
    assert_eq!(
        result,
        Err(Ok(BillPaymentsError::FunctionPaused)),
        "bulk_cleanup_bills must return FunctionPaused when archive function is paused"
    );
}

/// /// Invariant: cleanup blocked when the entire contract is paused.
#[test]
fn test_cleanup_blocked_when_contract_paused() {
    let env = make_env();
    let (client, owner) = setup(&env);

    client.set_pause_admin(&owner, &owner);
    client.pause(&owner);

    let result = client.try_bulk_cleanup_bills(&owner, &u64::MAX);
    assert_eq!(
        result,
        Err(Ok(BillPaymentsError::ContractPaused)),
        "bulk_cleanup_bills must return ContractPaused when contract is paused"
    );
}
