//! Regression tests for Issue #1741: bill scheduling and execution pagination
//! and cursor semantics.
//!
//! # Scope-safety fix
//!
//! `get_archived_bills`, `get_archived_bills_page`, `get_unpaid_bills_by_currency`,
//! and `get_bills_by_currency` all take an owner-scoped `owner: Address` parameter
//! but — unlike their sibling functions `get_unpaid_bills`, `get_all_bills_for_owner`,
//! `get_overdue_bills_for_owner`, and `get_bill_schedules_page` — never called
//! `owner.require_auth()`. Because every existing test in this crate sets up its
//! environment with `env.mock_all_auths()` (which auto-satisfies *any* address's
//! auth requirement regardless of who actually called), this gap was invisible to
//! the existing test suite. This file adds the missing negative-auth coverage and
//! fills in cursor/pagination coverage for `get_bills_by_currency`, which
//! previously had zero test coverage anywhere in the crate.
//!
//! # Test Coverage
//!
//! | Test | What it guards |
//! |------|-----------------|
//! | `get_archived_bills_requires_owner_auth` | scope-safety regression |
//! | `get_archived_bills_page_requires_owner_auth` | scope-safety regression |
//! | `get_unpaid_bills_by_currency_requires_owner_auth` | scope-safety regression |
//! | `get_bills_by_currency_requires_owner_auth` | scope-safety regression |
//! | `get_bills_by_currency_owner_isolation` | no cross-owner leakage under legitimate auth |
//! | `get_bills_by_currency_empty_page` | empty page, `next_cursor == 0` |
//! | `get_bills_by_currency_single_page` | fewer items than limit -> single page |
//! | `get_bills_by_currency_exact_boundary` | items == limit exactly -> no phantom next page |
//! | `get_bills_by_currency_invalid_cursor_past_end` | cursor beyond max ID -> empty page |
//! | `get_bills_by_currency_large_result_no_dup_no_gap` | multi-page traversal is a clean partition |
//! | `get_bills_by_currency_concurrent_insert_mid_pagination` | inserting a new bill between page fetches is picked up deterministically, without disturbing earlier pages |
//! | `get_bills_by_currency_includes_paid_bills` | unlike get_unpaid_bills_by_currency, paid bills are included |

use bill_payments::{BillPayments, BillPaymentsClient, ADMIN_ROTATION_TIMELOCK_SECONDS};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn make_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
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

/// `pay_bill` is gated by the cross-contract epoch guard: the caller must
/// present a configured trusted-orchestrator address and the current epoch.
/// This registers a throwaway orchestrator so `pay_bill` can be exercised in
/// isolation (irrelevant to the pagination behaviour under test).
fn pay_bill_for_test(env: &Env, client: &BillPaymentsClient, owner: &Address, bill_id: u32) {
    let admin = Address::generate(env);
    let orchestrator = Address::generate(env);
    client.init_admin(&admin, &ADMIN_ROTATION_TIMELOCK_SECONDS);
    client.set_trusted_orchestrator(&admin, &orchestrator);
    let epoch = client.get_cross_contract_epoch();
    client.pay_bill(&orchestrator, &epoch, owner, &bill_id);
}

fn create_bill_currency(
    env: &Env,
    client: &BillPaymentsClient,
    owner: &Address,
    currency: &str,
) -> u32 {
    client.create_bill(
        owner,
        &String::from_str(env, "Bill"),
        &100i128,
        &2_000_000_000u64,
        &false,
        &0u32,
        &None,
        &String::from_str(env, currency),
        &None,
    )
}

// ---------------------------------------------------------------------------
// Scope-safety: owner.require_auth() must be enforced (no mock_all_auths here)
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn get_archived_bills_requires_owner_auth() {
    let env = make_env();
    let (client, owner) = setup(&env);
    client.get_archived_bills(&owner, &0u32, &10u32);
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn get_archived_bills_page_requires_owner_auth() {
    let env = make_env();
    let (client, owner) = setup(&env);
    client.get_archived_bills_page(&owner, &0u32, &10u32);
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn get_unpaid_bills_by_currency_requires_owner_auth() {
    let env = make_env();
    let (client, owner) = setup(&env);
    let currency = String::from_str(&env, "USDC");
    client.get_unpaid_bills_by_currency(&owner, &currency, &0u32, &10u32);
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn get_bills_by_currency_requires_owner_auth() {
    let env = make_env();
    let (client, owner) = setup(&env);
    let currency = String::from_str(&env, "USDC");
    client.get_bills_by_currency(&owner, &currency, &0u32, &10u32);
}

// ---------------------------------------------------------------------------
// get_bills_by_currency: owner isolation under legitimate auth
// ---------------------------------------------------------------------------

#[test]
fn get_bills_by_currency_owner_isolation() {
    let env = make_env();
    env.mock_all_auths();
    let (client, alice) = setup(&env);
    let bob = Address::generate(&env);

    let alice_id = create_bill_currency(&env, &client, &alice, "USDC");
    let _bob_id = create_bill_currency(&env, &client, &bob, "USDC");

    let currency = String::from_str(&env, "USDC");
    let page = client.get_bills_by_currency(&alice, &currency, &0u32, &10u32);

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items.get(0).unwrap().id, alice_id);
    for bill in page.items.iter() {
        assert_eq!(bill.owner, alice, "bob's bill leaked into alice's page");
    }
}

// ---------------------------------------------------------------------------
// get_bills_by_currency: empty / single-page / boundary / invalid cursor
// ---------------------------------------------------------------------------

#[test]
fn get_bills_by_currency_empty_page() {
    let env = make_env();
    env.mock_all_auths();
    let (client, owner) = setup(&env);

    let currency = String::from_str(&env, "USDC");
    let page = client.get_bills_by_currency(&owner, &currency, &0u32, &10u32);

    assert_eq!(page.items.len(), 0);
    assert_eq!(page.count, 0);
    assert_eq!(page.next_cursor, 0);
}

#[test]
fn get_bills_by_currency_single_page() {
    let env = make_env();
    env.mock_all_auths();
    let (client, owner) = setup(&env);

    for _ in 0..5 {
        create_bill_currency(&env, &client, &owner, "USDC");
    }

    let currency = String::from_str(&env, "USDC");
    let page = client.get_bills_by_currency(&owner, &currency, &0u32, &10u32);

    assert_eq!(page.items.len(), 5);
    assert_eq!(page.count, 5);
    assert_eq!(
        page.next_cursor, 0,
        "fewer items than limit must be final page"
    );
}

#[test]
fn get_bills_by_currency_exact_boundary() {
    let env = make_env();
    env.mock_all_auths();
    let (client, owner) = setup(&env);

    // Exactly `limit` items must not produce a phantom next page.
    for _ in 0..10 {
        create_bill_currency(&env, &client, &owner, "USDC");
    }

    let currency = String::from_str(&env, "USDC");
    let page = client.get_bills_by_currency(&owner, &currency, &0u32, &10u32);

    assert_eq!(page.items.len(), 10);
    assert_eq!(page.count, 10);
    assert_eq!(
        page.next_cursor, 0,
        "item count exactly equal to limit must be the final page"
    );
}

#[test]
fn get_bills_by_currency_invalid_cursor_past_end() {
    let env = make_env();
    env.mock_all_auths();
    let (client, owner) = setup(&env);

    for _ in 0..3 {
        create_bill_currency(&env, &client, &owner, "USDC");
    }

    let currency = String::from_str(&env, "USDC");
    let page = client.get_bills_by_currency(&owner, &currency, &999_999u32, &10u32);

    assert_eq!(page.items.len(), 0);
    assert_eq!(page.count, 0);
    assert_eq!(page.next_cursor, 0);
}

// ---------------------------------------------------------------------------
// get_bills_by_currency: large-result multi-page traversal
// ---------------------------------------------------------------------------

#[test]
fn get_bills_by_currency_large_result_no_dup_no_gap() {
    let env = make_env();
    env.mock_all_auths();
    let (client, owner) = setup(&env);

    let mut expected: std::vec::Vec<u32> = std::vec::Vec::new();
    for i in 0..73 {
        let id = create_bill_currency(&env, &client, &owner, "USDC");
        expected.push(id);
        // Interleave unrelated-currency noise to prove the currency index
        // does not leak into the page.
        if i % 5 == 0 {
            create_bill_currency(&env, &client, &owner, "XLM");
        }
    }

    let currency = String::from_str(&env, "USDC");
    let mut collected: std::vec::Vec<u32> = std::vec::Vec::new();
    let mut cursor = 0u32;
    let mut prev_cursor = 0u32;
    let mut pages = 0u32;

    loop {
        let page = client.get_bills_by_currency(&owner, &currency, &cursor, &9u32);
        for bill in page.items.iter() {
            collected.push(bill.id);
            assert_eq!(bill.currency, currency);
        }
        pages += 1;
        assert!(page.items.len() <= 9);

        if page.next_cursor == 0 {
            break;
        }
        assert!(
            page.next_cursor > prev_cursor,
            "cursor must strictly advance: prev={prev_cursor} next={}",
            page.next_cursor
        );
        prev_cursor = page.next_cursor;
        cursor = page.next_cursor;
        assert!(pages < 100, "runaway pagination loop");
    }

    let mut collected_sorted = collected.clone();
    collected_sorted.sort_unstable();
    let mut dedup = collected_sorted.clone();
    dedup.dedup();
    assert_eq!(
        collected_sorted.len(),
        dedup.len(),
        "duplicate IDs returned across pages"
    );

    let mut expected_sorted = expected.clone();
    expected_sorted.sort_unstable();
    assert_eq!(
        collected_sorted, expected_sorted,
        "paginated union must exactly equal the expected USDC bill set"
    );
}

// ---------------------------------------------------------------------------
// get_bills_by_currency: concurrent insert between page fetches
// ---------------------------------------------------------------------------

/// Bill IDs are monotonically increasing and never reused, so a bill created
/// *after* page 1 was fetched must always sort after every ID already
/// returned. This proves a concurrent insert mid-pagination cannot cause the
/// cursor to re-return or skip an item.
#[test]
fn get_bills_by_currency_concurrent_insert_mid_pagination() {
    let env = make_env();
    env.mock_all_auths();
    let (client, owner) = setup(&env);

    let mut first_batch: std::vec::Vec<u32> = std::vec::Vec::new();
    for _ in 0..5 {
        first_batch.push(create_bill_currency(&env, &client, &owner, "USDC"));
    }

    let currency = String::from_str(&env, "USDC");
    let page1 = client.get_bills_by_currency(&owner, &currency, &0u32, &5u32);
    assert_eq!(page1.items.len(), 5);
    assert_eq!(
        page1.next_cursor, 0,
        "page 1 should be the final page so far"
    );

    // A new bill is created "concurrently" — i.e. between the caller's first
    // and second paginated reads.
    let inserted_id = create_bill_currency(&env, &client, &owner, "USDC");

    // Re-fetch from the start: the new bill must appear exactly once, after
    // all previously-seen items, and none of the original five may be
    // duplicated or dropped.
    let page2 = client.get_bills_by_currency(&owner, &currency, &0u32, &10u32);
    assert_eq!(page2.items.len(), 6);

    let mut ids: std::vec::Vec<u32> = page2.items.iter().map(|b| b.id).collect();
    ids.sort_unstable();
    let mut expected = first_batch.clone();
    expected.push(inserted_id);
    expected.sort_unstable();
    assert_eq!(ids, expected);

    // Resuming from page 1's cursor must return exactly the newly-inserted
    // bill and nothing else — no duplication of the first five.
    let page1_cursor = {
        let p = client.get_bills_by_currency(&owner, &currency, &0u32, &5u32);
        // With 6 items now present and limit=5, there must be a next page.
        assert_eq!(p.items.len(), 5);
        assert!(p.next_cursor > 0);
        p.next_cursor
    };
    let page2_resumed = client.get_bills_by_currency(&owner, &currency, &page1_cursor, &5u32);
    assert_eq!(page2_resumed.items.len(), 1);
    assert_eq!(page2_resumed.items.get(0).unwrap().id, inserted_id);
    assert_eq!(page2_resumed.next_cursor, 0);
}

// ---------------------------------------------------------------------------
// get_bills_by_currency: includes paid bills (contrast with the unpaid-only variant)
// ---------------------------------------------------------------------------

#[test]
fn get_bills_by_currency_includes_paid_bills() {
    let env = make_env();
    env.mock_all_auths();
    let (client, owner) = setup(&env);

    let unpaid_id = create_bill_currency(&env, &client, &owner, "USDC");
    let paid_id = create_bill_currency(&env, &client, &owner, "USDC");
    pay_bill_for_test(&env, &client, &owner, paid_id);

    let currency = String::from_str(&env, "USDC");
    let page = client.get_bills_by_currency(&owner, &currency, &0u32, &10u32);

    let mut ids: std::vec::Vec<u32> = page.items.iter().map(|b| b.id).collect();
    ids.sort_unstable();
    let mut expected = std::vec![unpaid_id, paid_id];
    expected.sort_unstable();
    assert_eq!(
        ids, expected,
        "get_bills_by_currency must include paid bills"
    );
}
